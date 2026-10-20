//! `runtime/manifest.json`, compiled into the host (design 5.10.3, 5.2.12).
//!
//! The host never reads the manifest in the app bundle while it runs: its
//! trust comes from the host binary's signature. This module parses the
//! fields the host uses; the runtime installer (T056) adds its own here.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde::Deserialize;

const TEXT: &str = include_str!("../../../runtime/manifest.json");

static EMBEDDED: LazyLock<Manifest> = LazyLock::new(|| {
    serde_json::from_str(TEXT).expect("the embedded manifest parses (checked by its unit test)")
});

/// The manifest the host was built with.
pub fn embedded() -> &'static Manifest {
    &EMBEDDED
}

/// The fields the host uses. Fields the host does not use are skipped.
#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub cli: Cli,
    pub env: EnvClasses,
    pub node: Pinned,
    pub claude_agent_acp: Pinned,
    /// The runtime's files by their path in the app bundle.
    pub files: BTreeMap<String, FileHash>,
}

/// The node of the runtime, as the manifest names it in `files`.
pub const NODE: &str = "Helpers/node";
/// claude-agent-acp's bundle, likewise.
pub const CLAUDE_BUNDLE: &str = "Resources/runtime/bundle-claude-acp.mjs";

/// A pinned component's version.
#[derive(Debug, Deserialize)]
pub struct Pinned {
    pub version: String,
}

/// What a runtime file must hash to before it runs (ADR-08).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FileHash {
    /// Lowercase hex.
    pub sha256: String,
    /// The code directory hash of a signed executable, lowercase hex.
    #[serde(default)]
    pub cdhash: Option<String>,
}

/// The CLI versions the pinned adapters were verified with (design 5.6.6).
#[derive(Debug, Deserialize)]
pub struct Cli {
    pub claude: CliRange,
    pub codex: CliRange,
}

/// Semantic version strings, as written; the version check parses them
/// (T094). `verified` is inclusive; below `unsupported_below` the CLI is
/// known to fail.
#[derive(Debug, Deserialize)]
pub struct CliRange {
    pub verified: VersionSpan,
    #[serde(default)]
    pub unsupported_below: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct VersionSpan {
    pub min: String,
    pub max: String,
}

/// Names, or prefixes ending in `*` (design 5.6.6). `allow` is the fixed
/// allowlist an adapter gets; `deny` is never passed on, whatever the
/// settings say. The host keeps no copy of either list.
#[derive(Debug, Deserialize)]
pub struct EnvClasses {
    pub allow: Vec<String>,
    pub deny: Vec<String>,
}

/// Whether `name` matches `pattern`: the same name, or for `PREFIX_*` any
/// name starting with `PREFIX_`.
pub fn env_pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_version(text: &str) -> bool {
        let parts: Vec<&str> = text.split('.').collect();
        parts.len() == 3 && parts.iter().all(|p| p.parse::<u64>().is_ok())
    }

    #[test]
    fn unit_env_rules_the_embedded_manifest_parses() {
        let manifest = embedded();
        for range in [&manifest.cli.claude, &manifest.cli.codex] {
            assert!(is_version(&range.verified.min), "{range:?}");
            assert!(is_version(&range.verified.max), "{range:?}");
            assert!(range.unsupported_below.as_deref().is_none_or(is_version));
        }
        assert!(manifest.env.allow.iter().any(|p| p == "PATH"));
        assert!(manifest.env.deny.iter().any(|p| p == "CUPPER_*"));
    }

    #[test]
    fn unit_env_rules_patterns_match_names_or_prefixes() {
        assert!(env_pattern_matches("LC_*", "LC_ALL"));
        assert!(env_pattern_matches("PATH", "PATH"));
        assert!(!env_pattern_matches("PATH", "PATHX"));
        assert!(!env_pattern_matches("NODE_*", "NODE"));
    }
}
