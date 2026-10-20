//! Findings (design 5.2.6): what in a scan's files the agents would load and
//! run, for the native confirmation only. They come from the scanned content
//! and change nothing in the digest; a file that cannot be parsed is listed as
//! `unparsed`, and the scan goes on.
//!
//! | File (at any level) | Findings |
//! |---|---|
//! | `.claude/settings*.json` | each hook command, each `env` key, `apiKeyHelper`, `statusLine` |
//! | `.mcp.json` | each server of `mcpServers` |
//! | `.claude/skills/<name>/SKILL.md` | the skill |
//! | `.claude/agents/**/*.md`, `.claude/commands/**/*.md` | the agent, the command |
//! | `.codex/**/*.toml` | each setting, as `key.path = <its text>` |
//!
//! `detail` shows what a file writes: strings as they are, other JSON values
//! in their RFC 8785 form, so no map order shows; TOML values as their source
//! text. JSON objects are walked in order of their keys.

use cupper_protocol::annotations::WirePath;
use cupper_protocol::canonical::to_canonical_string;
use cupper_protocol::methods::project::{Finding, FindingKind};
use serde_json::{Map, Value};
use toml_edit::{Document, Item, Table};

use super::{Scan, ScannedFile};

impl Scan {
    /// The findings of every file, in order of the files.
    pub fn findings(&self) -> Vec<Finding> {
        let mut findings = Vec::new();
        for file in &self.files {
            let mut found = |kind, detail| findings.push(finding(file, kind, detail));
            if let Err(why) = parse(file, &mut found) {
                found(FindingKind::Unparsed, why);
            }
        }
        findings
    }
}

fn finding(file: &ScannedFile, kind: FindingKind, detail: String) -> Finding {
    // APFS paths are UTF-8.
    let target = file.target.to_string_lossy().into_owned();
    Finding {
        kind,
        path: WirePath(file.path.clone()),
        target: file.linked.then_some(WirePath(target)),
        outside_workspace: file.outside_workspace,
        detail,
    }
}

type Found<'a> = dyn FnMut(FindingKind, String) + 'a;

/// The findings of `file`, or why it cannot be parsed.
fn parse(file: &ScannedFile, found: &mut Found) -> Result<(), String> {
    let mut local = file.path.as_str();
    while let Some(rest) = local.strip_prefix("../") {
        local = rest;
    }
    let text = || std::str::from_utf8(&file.content).map_err(|_| "not UTF-8".to_owned());
    let markdown = |under: &str| local.strip_prefix(under)?.strip_suffix(".md");
    if local == ".mcp.json" {
        return mcp(&object(text()?)?, found);
    }
    if let Some(name) = local.strip_prefix(".claude/settings")
        && name.ends_with(".json")
        && !name.contains('/')
    {
        return settings(&object(text()?)?, found);
    }
    if let Some(skill) = markdown(".claude/skills/").and_then(|s| s.strip_suffix("/SKILL")) {
        if !skill.contains('/') {
            found(FindingKind::Skill, skill.to_owned());
        }
    } else if let Some(agent) = markdown(".claude/agents/") {
        found(FindingKind::Agent, agent.to_owned());
    } else if let Some(command) = markdown(".claude/commands/") {
        found(FindingKind::Command, format!("/{command}"));
    } else if local.starts_with(".codex/") && local.ends_with(".toml") {
        let doc = Document::parse(text()?).map_err(|err| err.to_string())?;
        codex(doc.as_table(), doc.raw(), "", found);
    }
    Ok(())
}

fn object(text: &str) -> Result<Map<String, Value>, String> {
    match serde_json::from_str(text).map_err(|err| err.to_string())? {
        Value::Object(map) => Ok(map),
        _ => Err("not a JSON object".to_owned()),
    }
}

/// `map`'s entries in order of their keys.
fn sorted(map: &Map<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<_> = map.iter().collect();
    entries.sort_by_key(|(key, _)| *key);
    entries
}

/// A string as it is, any other value in its RFC 8785 form.
fn shown(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => to_canonical_string(other).unwrap_or_else(|_| other.to_string()),
    }
}

fn settings(map: &Map<String, Value>, found: &mut Found) -> Result<(), String> {
    for (key, value) in sorted(map) {
        match key.as_str() {
            "hooks" => hooks(value, found),
            "env" => match value {
                Value::Object(env) => {
                    for (name, value) in sorted(env) {
                        found(FindingKind::Env, format!("{name}={}", shown(value)));
                    }
                }
                other => found(FindingKind::Env, shown(other)),
            },
            "apiKeyHelper" => found(FindingKind::ApiKeyHelper, shown(value)),
            "statusLine" => found(FindingKind::StatusLine, command_of(value)),
            _ => {}
        }
    }
    Ok(())
}

/// `{"<event>": [{"matcher"?, "hooks": [{"type", "command"}]}]}`: one finding
/// per hook, `<event> <matcher>: <command>`; anything else shown whole.
fn hooks(value: &Value, found: &mut Found) {
    let Value::Object(events) = value else {
        return found(FindingKind::Hook, shown(value));
    };
    for (event, groups) in sorted(events) {
        let Value::Array(groups) = groups else {
            found(FindingKind::Hook, format!("{event}: {}", shown(groups)));
            continue;
        };
        for group in groups {
            let matcher = match group.get("matcher").map(shown).as_deref() {
                None | Some("") => event.clone(),
                Some(matcher) => format!("{event} {matcher}"),
            };
            match group.get("hooks") {
                Some(Value::Array(hooks)) => {
                    for hook in hooks {
                        found(
                            FindingKind::Hook,
                            format!("{matcher}: {}", command_of(hook)),
                        );
                    }
                }
                _ => found(FindingKind::Hook, format!("{matcher}: {}", shown(group))),
            }
        }
    }
}

/// The `command` of an object, or the value shown whole.
fn command_of(value: &Value) -> String {
    match value.get("command") {
        Some(Value::String(command)) => command.clone(),
        _ => shown(value),
    }
}

/// `{"mcpServers": {"<name>": {"command", "args"} | {"url"}}}`.
fn mcp(map: &Map<String, Value>, found: &mut Found) -> Result<(), String> {
    let Some(servers) = map.get("mcpServers") else {
        return Ok(());
    };
    let Value::Object(servers) = servers else {
        found(FindingKind::McpServer, shown(servers));
        return Ok(());
    };
    for (name, server) in sorted(servers) {
        let runs = match (server.get("command"), server.get("url")) {
            (Some(Value::String(command)), _) => {
                let args = server.get("args").and_then(Value::as_array);
                let args = args
                    .into_iter()
                    .flatten()
                    .map(|arg| format!(" {}", shown(arg)));
                format!("{command}{}", args.collect::<String>())
            }
            (_, Some(Value::String(url))) => url.clone(),
            _ => shown(server),
        };
        found(FindingKind::McpServer, format!("{name}: {runs}"));
    }
    Ok(())
}

/// Every setting under `table`, `<key.path> = <its text in raw>`, in the
/// order the file writes them.
fn codex(table: &Table, raw: &str, prefix: &str, found: &mut Found) {
    for (key, item) in table.iter() {
        let path = format!("{prefix}{key}");
        match item {
            Item::Table(table) => codex(table, raw, &format!("{path}."), found),
            Item::ArrayOfTables(tables) => {
                for (i, table) in tables.iter().enumerate() {
                    codex(table, raw, &format!("{path}[{i}]."), found);
                }
            }
            Item::Value(_) | Item::None => {
                let text = item.span().and_then(|span| raw.get(span)).unwrap_or("");
                found(FindingKind::CodexConfig, format!("{path} = {text}"));
            }
        }
    }
}

/// UNIT-trust-findings, on scans made in memory; every name is made up.
#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use cupper_protocol::ids::Digest;

    use super::*;

    fn file(path: &str, content: &[u8]) -> ScannedFile {
        ScannedFile {
            path: path.to_owned(),
            target: PathBuf::from("/work/app").join(path),
            linked: false,
            outside_workspace: false,
            hash: [0; 32],
            content: content.to_vec(),
        }
    }

    fn found(files: Vec<ScannedFile>) -> Vec<(FindingKind, String)> {
        let scan = Scan {
            files,
            digest: Digest::from_sha256(&[0; 32]),
        };
        let findings = scan.findings();
        findings.into_iter().map(|f| (f.kind, f.detail)).collect()
    }

    fn of(kind: FindingKind, detail: &str) -> (FindingKind, String) {
        (kind, detail.to_owned())
    }

    #[test]
    fn unit_trust_findings_settings_list_each_kind_in_one_file() {
        let settings = br#"{
            "statusLine": {"type": "command", "command": "acme-status --short"},
            "hooks": {
                "Stop": [{"hooks": [{"type": "command", "command": "acme-notify done"}]}],
                "PreToolUse": [{"matcher": "Bash", "hooks": [
                    {"type": "command", "command": "acme-lint"},
                    {"type": "prompt", "prompt": "Check it"}
                ]}]
            },
            "env": {"ACME_MODE": "strict", "ACME_LEVEL": 2},
            "apiKeyHelper": "/opt/acme/key-helper.sh",
            "model": "not a finding"
        }"#;
        let local = br#"{"env": {"ACME_DEBUG": "1"}}"#;
        let files = vec![
            file("../.claude/settings.local.json", local),
            file(".claude/settings.json", settings),
        ];
        use FindingKind::*;
        assert_eq!(
            found(files),
            [
                of(Env, "ACME_DEBUG=1"),
                of(ApiKeyHelper, "/opt/acme/key-helper.sh"),
                of(Env, "ACME_LEVEL=2"),
                of(Env, "ACME_MODE=strict"),
                of(Hook, "PreToolUse Bash: acme-lint"),
                of(
                    Hook,
                    r#"PreToolUse Bash: {"prompt":"Check it","type":"prompt"}"#
                ),
                of(Hook, "Stop: acme-notify done"),
                of(StatusLine, "acme-status --short"),
            ]
        );
    }

    #[test]
    fn unit_trust_findings_mcp_servers_skills_agents_and_commands() {
        let mcp = br#"{"mcpServers": {
            "zeta": {"command": "npx", "args": ["-y", "acme-mcp"]},
            "alpha": {"type": "http", "url": "https://mcp.example.invalid/sse"},
            "odd": {"transport": 1}
        }}"#;
        let files = vec![
            file(".claude/agents/team/reviewer.md", b"---"),
            file(".claude/commands/deploy.md", b"Deploy"),
            file(".claude/notes.txt", b"not a finding"),
            file(".claude/skills/review/SKILL.md", b"---"),
            file(".claude/skills/review/run.sh", b"echo"),
            file(".mcp.json", mcp),
        ];
        use FindingKind::*;
        assert_eq!(
            found(files),
            [
                of(Agent, "team/reviewer"),
                of(Command, "/deploy"),
                of(Skill, "review"),
                of(McpServer, "alpha: https://mcp.example.invalid/sse"),
                of(McpServer, r#"odd: {"transport":1}"#),
                of(McpServer, "zeta: npx -y acme-mcp"),
            ]
        );
    }

    #[test]
    fn unit_trust_findings_codex_settings_with_their_text() {
        let config = br#"model = "acme-model"
notify = ["acme-notify", "--done"]

[mcp_servers.acme]
command = "acme-mcp"
env = { ACME_TOKEN = "placeholder" }

[[profiles]]
name = "fast"
"#;
        let files = vec![file("../../.codex/config.toml", config)];
        let codex = |detail| of(FindingKind::CodexConfig, detail);
        assert_eq!(
            found(files),
            [
                codex(r#"model = "acme-model""#),
                codex(r#"notify = ["acme-notify", "--done"]"#),
                codex(r#"mcp_servers.acme.command = "acme-mcp""#),
                codex(r#"mcp_servers.acme.env = { ACME_TOKEN = "placeholder" }"#),
                codex(r#"profiles[0].name = "fast""#),
            ]
        );
    }

    #[test]
    fn unit_trust_findings_unparsed_files_are_listed_and_the_rest_goes_on() {
        let files = vec![
            file(".claude/agents/reviewer.md", b"---"),
            file(".claude/settings.json", b"[]"),
            file(".claude/settings.local.json", b"\xff\xfe"),
            file(".codex/config.toml", b"model = "),
            file(".mcp.json", b"{not json"),
        ];
        let found = found(files);
        assert_eq!(found[0], of(FindingKind::Agent, "reviewer"));
        assert_eq!(found[1], of(FindingKind::Unparsed, "not a JSON object"));
        assert_eq!(found[2], of(FindingKind::Unparsed, "not UTF-8"));
        for (kind, detail) in &found[3..] {
            assert_eq!(*kind, FindingKind::Unparsed, "{detail}");
            assert!(!detail.is_empty());
        }
        assert_eq!(found.len(), 5);
    }

    #[test]
    fn unit_trust_findings_carry_the_link_target_outside_the_workspace() {
        let mut linked = file(
            ".mcp.json",
            br#"{"mcpServers": {"acme": {"url": "https://x.invalid"}}}"#,
        );
        linked.linked = true;
        linked.outside_workspace = true;
        linked.target = PathBuf::from("/work/dotfiles/mcp.json");
        let own = file(".claude/agents/reviewer.md", b"---");
        let scan = Scan {
            files: vec![own, linked],
            digest: Digest::from_sha256(&[0; 32]),
        };
        let findings = scan.findings();
        assert_eq!(
            (findings[0].target.as_ref(), findings[0].outside_workspace),
            (None, false)
        );
        let outside = &findings[1];
        assert_eq!(outside.path.0, ".mcp.json");
        assert_eq!(
            outside.target.as_ref().unwrap().0,
            "/work/dotfiles/mcp.json"
        );
        assert!(outside.outside_workspace);
    }
}
