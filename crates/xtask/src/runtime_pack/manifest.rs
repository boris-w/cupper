//! `runtime/manifest.json` (design 5.10.3). Maintained by hand: the pinned
//! versions, addresses and hashes of the components, and the classes of
//! environment variables (design 5.6.6). Written by `runtime-pack`: `files`,
//! the hash of every file the host takes from the app bundle (paths relative
//! to `CupperHost.app/Contents`, design 5.10.1), the claude-agent-acp bundle
//! hash and the variables each adapter reads. Keys are in a fixed order (struct
//! fields, sorted maps), so the file does not depend on serde_json's map order.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::envscan::Reads;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub node: Node,
    pub codex_acp: NpmPackage,
    pub esbuild_wasm: NpmPackage,
    pub claude_agent_acp: ClaudeAgentAcp,
    pub env: Env,
    pub files: BTreeMap<String, FileHash>,
}

/// The official darwin-arm64 tarball; `bin/node` is re-signed with `identifier`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub version: String,
    pub url: String,
    pub sha256: String,
    pub identifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NpmPackage {
    pub version: String,
    pub url: String,
    pub integrity: String,
}

/// Installed on the user's machine from `lockfile` (relative to `runtime/`
/// here and to `Resources/runtime/` in the app), then bundled from `entry`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClaudeAgentAcp {
    pub version: String,
    pub lockfile: String,
    pub entry: String,
    pub bundle_sha256: String,
}

/// Classes are names or prefixes ending in `*`: `allow`, the fixed allowlist
/// the host passes on; `deny`, never passed on, whatever the settings say;
/// `harmless`, read by an adapter but neither set by the host nor dangerous
/// when a user passes it on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Env {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
    pub harmless: Vec<String>,
    pub reads: BTreeMap<String, Reads>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileHash {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cdhash: Option<String>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let text =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))
    }

    pub fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(self).expect("the manifest serializes");
        text.push('\n');
        text
    }
}

/// Where `got` differs from `want`, one line per field; lists show the
/// entries that were added (`+`) or are missing (`-`).
pub fn differences(want: &Manifest, got: &Manifest) -> Vec<String> {
    let mut found = Vec::new();
    let value = |manifest| serde_json::to_value(manifest).expect("the manifest serializes");
    compare("", &value(want), &value(got), &mut found);
    found
}

fn compare(path: &str, want: &Value, got: &Value, found: &mut Vec<String>) {
    match (want, got) {
        (Value::Object(want), Value::Object(got)) => {
            let keys: BTreeSet<&String> = want.keys().chain(got.keys()).collect();
            for key in keys {
                let (a, b) = (want.get(key.as_str()), got.get(key.as_str()));
                compare(
                    &format!("{path}/{key}"),
                    a.unwrap_or(&Value::Null),
                    b.unwrap_or(&Value::Null),
                    found,
                );
            }
        }
        (Value::Array(want), Value::Array(got)) => {
            let added = got.iter().filter(|item| !want.contains(item));
            let missing = want.iter().filter(|item| !got.contains(item));
            found.extend(added.map(|item| format!("{path}: +{item}")));
            found.extend(missing.map(|item| format!("{path}: -{item}")));
        }
        _ if want != got => found.push(format!("{path}: {want} -> {got}")),
        _ => {}
    }
}
