//! The real configuration before and after a real run (U-R4, security #8),
//! as T003's `config-hash.ts` compared it: the settings files whole, and of
//! `~/.claude.json`, which the CLI rewrites as it goes, only the fields that
//! grant permissions or start programs. Only digests are kept; the report
//! names what changed, never a value or a path. Credentials are not read.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::report::ConfigCheck;

/// Compared whole, under the home directory.
const FILES: &str = ".claude/settings.json .claude/settings.local.json .codex/config.toml";
/// Compared by field: the top-level ones, besides any whose name looks like
/// one of `PERMISSION`, and those of each project entry.
const STATE: &str = ".claude.json";
const TOP: &str = "mcpServers bypassPermissionsModeAccepted autoModeEnvSetup";
const PERMISSION: &str = "permission bypass automode trust allowed";
const PROJECT: &str = "allowedTools hasTrustDialogAccepted mcpServers enabledMcpServers \
    disabledMcpServers enabledMcpjsonServers disabledMcpjsonServers \
    hasClaudeMdExternalIncludesApproved";

/// What `--dry-run` lists.
pub fn names() -> String {
    let files = FILES.split(' ').map(|file| format!("~/{file}"));
    let state = format!("~/{STATE} (the fields that grant permissions or start programs)");
    files.chain([state]).collect::<Vec<_>>().join(", ")
}

/// Digests by name, and of each project entry of `~/.claude.json` by its
/// path; what is missing digests as `absent`.
#[derive(Debug)]
pub struct Snapshot {
    entries: BTreeMap<String, String>,
    projects: BTreeMap<String, Vec<String>>,
}

pub fn snapshot(home: &Path) -> Result<Snapshot> {
    let hex = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let digest = |value: &Value| match value {
        Value::Null => "absent".to_owned(),
        value => hex(value.to_string().as_bytes()),
    };
    let mut entries = BTreeMap::new();
    for file in FILES.split(' ') {
        let digest = std::fs::read(home.join(file)).map_or("absent".into(), |b| hex(&b));
        entries.insert(format!("~/{file}"), digest);
    }
    let state: Value = match std::fs::read_to_string(home.join(STATE)) {
        Ok(text) => serde_json::from_str(&text).context("read ~/.claude.json")?,
        Err(_) => Value::Null,
    };
    let keys = state.as_object().into_iter().flat_map(|top| top.keys());
    let looks = |key: &&String| {
        PERMISSION
            .split(' ')
            .any(|w| key.to_lowercase().contains(w))
    };
    for key in TOP.split(' ').chain(keys.filter(looks).map(String::as_str)) {
        entries.insert(format!("~/{STATE} {key}"), digest(&state[key]));
    }
    let fields = |p: &Value| PROJECT.split_whitespace().map(|f| digest(&p[f])).collect();
    let projects = state["projects"].as_object().into_iter().flatten();
    let projects = projects
        .map(|(path, p)| (path.clone(), fields(p)))
        .collect();
    Ok(Snapshot { entries, projects })
}

/// What the run changed, and what it left to clean up: the new project
/// entries of its own temporary repositories, under `ours`. Any other new
/// entry is a change.
pub fn compare(
    before: &Snapshot,
    after: &Snapshot,
    ours: &[PathBuf],
) -> (Vec<ConfigCheck>, Vec<String>) {
    let check = |name: String, changed| ConfigCheck { name, changed };
    let names: BTreeSet<_> = before.entries.keys().chain(after.entries.keys()).collect();
    let entry = |name: &String| before.entries.get(name) != after.entries.get(name);
    let mut checks: Vec<_> = names
        .into_iter()
        .map(|name| check(name.clone(), entry(name)))
        .collect();
    for (n, field) in PROJECT.split_whitespace().enumerate() {
        let now = |path| after.projects.get(path).map(|now: &Vec<String>| &now[n]);
        let changed = before
            .projects
            .iter()
            .any(|(path, was)| now(path) != Some(&was[n]));
        checks.push(check(format!("~/{STATE} projects.*.{field}"), changed));
    }
    let added = after
        .projects
        .keys()
        .filter(|path| !before.projects.contains_key(*path));
    let ours = |path: &&String| ours.iter().any(|root| Path::new(path).starts_with(root));
    let (own, other): (Vec<_>, Vec<_>) = added.partition(ours);
    let name = format!("~/{STATE} projects added outside this run's temporary repositories");
    checks.push(check(name, !other.is_empty()));
    let n = own.len();
    let cleanup = format!("~/{STATE}: {n} project entries of this run's temporary repositories");
    (checks, (n > 0).then_some(cleanup).into_iter().collect())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::json;

    use super::*;
    use crate::testutil::TempDir;

    fn changed(checks: &[ConfigCheck]) -> Vec<&str> {
        checks
            .iter()
            .filter(|c| c.changed)
            .map(|c| c.name.as_str())
            .collect()
    }

    #[test]
    fn unit_compat_guards_the_real_configuration_is_compared_by_name() {
        let home = TempDir::new("compat-home");
        let (claude, ours) = (home.path().join(".claude"), home.path().join("tmp"));
        fs::create_dir_all(&claude).unwrap();
        fs::write(claude.join("settings.json"), "{}").unwrap();
        let state = |extra: serde_json::Value| {
            let mut state = json!({"numStartups": 1, "bypassPermissionsModeAccepted": false,
                "projects": {"/work/a": {"allowedTools": [], "history": []}}});
            state
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            fs::write(home.path().join(".claude.json"), state.to_string()).unwrap();
            snapshot(home.path()).unwrap()
        };
        let before = state(json!({}));
        // The CLI's own bookkeeping, and an entry of this run's repository.
        let own = format!("{}/repo", ours.display());
        let after = state(
            json!({"numStartups": 2, "projects": {"/work/a": {"allowedTools": [],
            "history": ["x"]}, &own: {"allowedTools": []}}}),
        );
        let (checks, cleanup) = compare(&before, &after, std::slice::from_ref(&ours));
        assert_eq!(changed(&checks), Vec::<&str>::new());
        assert_eq!(cleanup.len(), 1);
        assert!(!cleanup[0].contains(&own), "no paths in the report");

        fs::write(claude.join("settings.json"), "{\"model\": \"opus\"}").unwrap();
        let after = state(json!({"skipDangerousModePermissionPrompt": true,
            "projects": {"/work/a": {"allowedTools": ["Bash"]}, "/work/b": {}}}));
        let (checks, cleanup) = compare(&before, &after, &[ours]);
        assert_eq!(
            changed(&checks),
            [
                "~/.claude.json skipDangerousModePermissionPrompt",
                "~/.claude/settings.json",
                "~/.claude.json projects.*.allowedTools",
                "~/.claude.json projects added outside this run's temporary repositories",
            ]
        );
        assert!(cleanup.is_empty());
        assert!(
            checks
                .iter()
                .all(|c| !c.name.contains("opus") && !c.name.contains("Bash"))
        );
    }
}
