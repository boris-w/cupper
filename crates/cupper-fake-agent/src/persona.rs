//! What each persona looks like on the wire, after the SP-1 recordings of
//! claude-agent-acp 0.91.0 and codex-acp 2.2.2 (`spikes/SP-1-adapter-behavior/
//! recordings/`; the scenario is named on each item). The static parts live in
//! `personas/<persona>.json`; this module loads them and builds the shapes
//! that carry ids, paths or text. Content that depends on the recording
//! machine is replaced: the model list is cut down, account details are
//! placeholders.

use std::path::Path;
use std::sync::LazyLock;

use agent_client_protocol::Error;
use serde_json::{Value, json};

use crate::config::Persona::{self, Claude, Codex};

static CLAUDE: LazyLock<Value> = LazyLock::new(|| load(include_str!("../personas/claude.json")));
static CODEX: LazyLock<Value> = LazyLock::new(|| load(include_str!("../personas/codex.json")));

fn load(text: &str) -> Value {
    serde_json::from_str(text).expect("persona data is valid JSON (unit-tested)")
}

/// The persona's static data.
pub fn data(p: Persona) -> &'static Value {
    match p {
        Claude => &CLAUDE,
        Codex => &CODEX,
    }
}

/// `initialize` result (s01). `steering.supported` sits in the top-level `_meta` (T003).
pub fn initialize(p: Persona) -> Value {
    data(p)["initialize"].clone()
}

/// `_auth/status_update` params (s01, s10). Claude's carries the account (K16).
pub fn auth_status(p: Persona, logged_in: bool) -> Value {
    let status = if logged_in {
        data(p)["authStatus"].clone()
    } else {
        json!({"kind": "none", "label": "Not logged in"})
    };
    json!({"authStatus": status})
}

/// The session's config options as `session/new` returns them (s01).
pub fn config_options(p: Persona) -> Vec<Value> {
    data(p)["configOptions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// The result of `session/new`, `session/resume` and `session/load` (s01,
/// s07b, s09): the legacy `modes` mirror the `mode` option; Codex adds the
/// legacy `models`.
pub fn session_result(p: Persona, session: Option<&str>, config: &[Value]) -> Value {
    let mode = config
        .iter()
        .find(|o| o["id"] == "mode")
        .cloned()
        .unwrap_or_default();
    let modes: Vec<Value> = mode["options"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|o| json!({"id": o["value"], "name": o["name"], "description": o["description"]}))
        .collect();
    let mut result = json!({});
    if let Some(id) = session {
        result["sessionId"] = json!(id);
    }
    result["modes"] = json!({"currentModeId": mode["currentValue"], "availableModes": modes});
    result["configOptions"] = json!(config);
    if p == Codex {
        result["models"] = data(p)["models"].clone();
    }
    result
}

/// The slash commands, pushed after `session/new` and at the start of every
/// prompt (s01; the user's own skills left out).
pub fn commands(p: Persona) -> Value {
    json!({"sessionUpdate": "available_commands_update", "availableCommands": data(p)["commands"]})
}

/// `session/request_permission` params for a command (`execute`) or a file
/// edit (any other kind); the options as in s04 and s04b.
pub fn permission(p: Persona, session: &str, tool: &str, kind: &str, cwd: &Path) -> Value {
    let execute = kind == "execute";
    let file = cwd.join("fake.txt");
    let call = match (p, execute) {
        (Claude, true) => json!({"toolCallId": tool, "name": "Bash", "status": "pending",
            "rawInput": {"command": "fake command"}, "title": "fake command", "kind": "execute"}),
        (Claude, false) => json!({"toolCallId": tool, "name": "Write", "status": "pending",
            "title": format!("Write {}", file.display()), "kind": "edit"}),
        (Codex, true) => json!({"toolCallId": tool, "title": "Run command",
            "rawInput": {"command": "fake command", "cwd": cwd}, "kind": "execute", "status": "pending"}),
        (Codex, false) => json!({"toolCallId": tool, "title": "Edit files",
            "locations": [{"path": file}], "kind": "edit", "status": "pending"}),
    };
    let options = &data(p)["permissionOptions"][if execute { "execute" } else { "edit" }];
    json!({"sessionId": session, "toolCall": call, "options": options})
}

/// Whether `option` is the persona's rejection: Claude's turn goes on after
/// it, Codex's ends (#556, K2).
pub fn rejects(p: Persona, option: &str) -> bool {
    let options = data(p)["permissionOptions"]["execute"].as_array();
    options
        .into_iter()
        .flatten()
        .any(|o| o["optionId"] == option && o["kind"] == "reject_once")
}

/// Plan confirmation (s05): Claude's ExitPlanMode, Codex's "Implement this plan?".
pub fn plan_confirm(p: Persona, session: &str, plan: &str) -> Value {
    let call = match p {
        Claude => {
            json!({"toolCallId": "fake-exit-plan", "name": "ExitPlanMode", "status": "pending",
            "rawInput": {"plan": plan}, "title": "Approve Plan", "kind": "switch_mode",
            "content": [{"type": "content", "content": {"type": "text", "text": plan}}]})
        }
        Codex => json!({"toolCallId": format!("plan-review:{session}-plan"), "kind": "switch_mode",
            "status": "pending", "title": "Implement this plan?", "rawInput": {"plan": plan}}),
    };
    json!({"sessionId": session, "toolCall": call, "options": data(p)["planOptions"]})
}

/// `plan_update` (unstable, Markdown; s05).
pub fn plan_update(p: Persona, session: &str, markdown: &str) -> Value {
    let id = if p == Claude {
        "plan-mode".to_owned()
    } else {
        format!("{session}-plan")
    };
    let plan = json!({"type": "markdown", "planId": id, "content": markdown});
    json!({"sessionUpdate": "plan_update", "plan": plan})
}

/// A multi-question form (s05): Claude's AskUserQuestion has `question_N` and
/// `question_N_custom`; Codex's request_user_input keys by question with a
/// `<question>_note` field.
pub fn form(p: Persona, session: &str, questions: &[(&str, Vec<&str>)]) -> Value {
    let mut props = serde_json::Map::new();
    for (i, (q, options)) in questions.iter().enumerate() {
        let one_of: Vec<Value> = options
            .iter()
            .map(|o| json!({"const": o, "title": o}))
            .collect();
        match p {
            Claude => {
                let custom =
                    "Type your own answer, or add a note to the option you chose above (optional).";
                props.insert(
                    format!("question_{i}"),
                    json!({"type": "string", "title": q, "oneOf": one_of}),
                );
                props.insert(
                    format!("question_{i}_custom"),
                    json!({"type": "string", "title": "Other", "description": custom}),
                );
            }
            Codex => {
                let meta = json!({"codex": {"isOther": true, "isSecret": false}});
                let note =
                    json!({"codex": {"questionId": q, "role": "user_note", "isSecret": false}});
                props.insert(
                    q.to_string(),
                    json!({"title": q, "description": q, "_meta": meta, "type": "string", "oneOf": one_of}),
                );
                props.insert(
                    format!("{q}_note"),
                    json!({"type": "string", "title": "Additional answer or note", "_meta": note}),
                );
            }
        }
    }
    let names: Vec<&str> = questions.iter().map(|(q, _)| *q).collect();
    match p {
        Claude => json!({"mode": "form", "sessionId": session, "message": names.join(" / "),
            "requestedSchema": {"type": "object", "properties": props}}),
        Codex => {
            json!({"sessionId": session, "mode": "form", "message": "Codex needs your input to continue.",
            "requestedSchema": {"type": "object", "properties": props, "required": names},
            "_meta": {"codex": {"autoResolutionMs": null}}})
        }
    }
}

/// The start of a command: Claude's Bash is pending with no output yet; Codex's
/// exec_command is in progress (s03, s07).
pub fn terminal_start(p: Persona, id: &str, cwd: &Path) -> Value {
    let command = format!("fake-command {id}");
    let content = json!([{"type": "terminal", "terminalId": id}]);
    match p {
        Claude => {
            json!({"_meta": {"claudeCode": {"toolName": "Bash"}, "terminal_info": {"terminal_id": id}},
            "toolCallId": id, "sessionUpdate": "tool_call", "name": "Bash", "rawInput": {"command": command},
            "status": "pending", "title": command, "kind": "execute", "content": content})
        }
        Codex => json!({"sessionUpdate": "tool_call", "toolCallId": id, "name": "exec_command",
            "kind": "execute", "title": command, "status": "in_progress", "content": content,
            "rawInput": {"command": command, "cwd": cwd},
            "_meta": {"terminal_info": {"cwd": cwd, "terminal_id": id}}}),
    }
}

/// Command output in `_meta.terminal_output` (Zed's convention; s03).
pub fn terminal_output(id: &str, data: &str) -> Value {
    let meta = json!({"terminal_output": {"terminal_id": id, "data": data}});
    json!({"sessionUpdate": "tool_call_update", "toolCallId": id, "_meta": meta})
}

/// The end of a command in `_meta.terminal_exit`; failed on a non-zero code (s03).
pub fn terminal_exit(id: &str, code: i64) -> Value {
    let status = if code == 0 { "completed" } else { "failed" };
    let meta = json!({"terminal_exit": {"terminal_id": id, "exit_code": code, "signal": null}});
    json!({"sessionUpdate": "tool_call_update", "toolCallId": id, "status": status, "_meta": meta})
}

/// An edit with a diff (s02): Claude's diff is an excerpt with a location;
/// Codex sends one diff per hunk, tagged in `_meta.kind`.
pub fn diff(p: Persona, id: &str, path: &Path) -> Value {
    let mut diff =
        json!({"type": "diff", "path": path, "oldText": "old line\n", "newText": "new line\n"});
    match p {
        Claude => json!({"_meta": {"claudeCode": {"toolName": "Edit"}}, "toolCallId": id,
            "sessionUpdate": "tool_call", "name": "Edit", "status": "pending",
            "title": format!("Edit {}", path.display()), "kind": "edit", "content": [diff],
            "locations": [{"path": path, "line": 1}]}),
        Codex => {
            diff["_meta"] = json!({"kind": "update"});
            json!({"sessionUpdate": "tool_call", "toolCallId": id, "kind": "edit",
                "title": "Editing files", "status": "in_progress", "content": [diff]})
        }
    }
}

/// Codex's run state in `session_info_update._meta.codex.threadStatus` (s02, s05).
pub fn thread_status(kind: &str, flags: &[&str]) -> Value {
    let status = if kind == "active" {
        json!({"type": kind, "activeFlags": flags})
    } else {
        json!({"type": kind})
    };
    json!({"sessionUpdate": "session_info_update", "_meta": {"codex": {"threadStatus": status}}})
}

/// Codex's automatic retry after an upstream error (K18).
pub fn upstream_retry(attempt: u64) -> Value {
    let details = "stream disconnected before completion: \
        Our servers are currently overloaded. Please try again later.";
    let error = json!({"message": format!("Reconnecting... {attempt}/5"),
        "codexErrorInfo": {"responseStreamDisconnected": {"httpStatusCode": null}},
        "additionalDetails": details, "misalignment": null, "turnId": "fake-turn", "willRetry": true});
    json!({"sessionUpdate": "session_info_update", "_meta": {"codex": {"error": error}}})
}

/// `-32000` (s10).
pub fn auth_required() -> Error {
    Error::new(-32000, "Authentication required")
}

/// Claude after its CLI died: every request on the session (s07b, K6).
pub fn session_not_found() -> Error {
    Error::internal_error().data(json!({"details": "Session not found"}))
}

/// Claude's prompt when the CLI dies mid-turn (s07b, K6).
pub fn cli_crashed() -> Error {
    let message = "Internal error: The Claude Agent process exited unexpectedly. \
        Please start a new session.";
    Error::new(-32603, message)
}

/// Codex's prompt when the app-server dies; the next request restarts it (s07, K7).
pub fn app_server_killed() -> Error {
    let message = "The Codex app-server was killed by SIGKILL, which usually means it ran out \
        of memory. The agent starts it again on the next request.";
    Error::new(1001, message)
        .data(json!({"exitCode": null, "signal": "SIGKILL", "restartable": true}))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected values read from the SP-1 recordings with the jq commands in
    // the T051 notes (s01, s04, s04b, s05).

    fn ids<'a>(list: &'a Value, key: &str) -> Vec<&'a str> {
        list.as_array()
            .into_iter()
            .flatten()
            .filter_map(|o| o[key].as_str())
            .collect()
    }

    fn option_ids(p: Persona, kind: &str) -> Vec<&'static str> {
        let options = &data(p)["permissionOptions"][kind];
        ids(options, "optionId")
    }

    #[test]
    fn unit_fake_scripts_persona_claude_matches_sp1() {
        let init = initialize(Claude);
        assert_eq!(init["_meta"]["steering"]["supported"], true);
        assert_eq!(
            init["agentCapabilities"]["_meta"]["claudeCode"]["promptQueueing"],
            true
        );
        assert_eq!(
            init["agentCapabilities"]["mcpCapabilities"],
            json!({"http": true, "sse": true})
        );
        let config = Value::from(config_options(Claude));
        assert_eq!(ids(&config, "id"), ["mode", "model", "effort"]);
        assert_eq!(ids(&config, "category"), ["mode", "model", "thought_level"]);
        let modes = [
            "default",
            "acceptEdits",
            "plan",
            "auto",
            "bypassPermissions",
        ];
        assert_eq!(ids(&config[0]["options"], "value"), modes);
        assert_eq!(config[0]["currentValue"], "default");
        assert_eq!(
            option_ids(Claude, "execute"),
            ["allow-once", "allow-with-updates", "reject"]
        );
        assert_eq!(
            option_ids(Claude, "edit"),
            ["allow-once", "allow-with-updates", "reject"]
        );
        let plan = [
            "exit-plan-default",
            "exit-plan-clear-auto",
            "exit-plan-auto",
            "exit-plan-bypass",
            "reject",
        ];
        assert_eq!(ids(&data(Claude)["planOptions"], "optionId"), plan);
        assert!(rejects(Claude, "reject") && !rejects(Claude, "allow-once"));
    }

    #[test]
    fn unit_fake_scripts_persona_codex_matches_sp1() {
        let init = initialize(Codex);
        assert_eq!(init["_meta"]["steering"]["supported"], true);
        assert!(
            init["agentCapabilities"]["_meta"]
                .get("claudeCode")
                .is_none()
        );
        assert!(init["agentCapabilities"]["sessionCapabilities"]["subagents"].is_object());
        let config = Value::from(config_options(Codex));
        assert_eq!(
            ids(&config, "id"),
            ["mode", "collaboration_mode", "model", "reasoning_effort"]
        );
        assert_eq!(
            ids(&config, "category"),
            ["mode", "collaboration_mode", "model", "thought_level"]
        );
        let modes = ["read-only", "workspace-write", "agent", "agent-full-access"];
        assert_eq!(ids(&config[0]["options"], "value"), modes);
        assert_eq!(config[0]["currentValue"], "agent");
        let execute = ["allow_once", "accept_execpolicy_amendment", "cancel"];
        assert_eq!(option_ids(Codex, "execute"), execute);
        assert_eq!(
            option_ids(Codex, "edit"),
            ["allow_once", "allow_for_session", "cancel"]
        );
        assert_eq!(
            ids(&data(Codex)["planOptions"], "optionId"),
            ["implement_plan", "revise_plan"]
        );
        assert!(rejects(Codex, "cancel") && !rejects(Codex, "allow_once"));
        assert!(session_result(Codex, None, &[])["models"]["availableModels"].is_array());
    }
}
