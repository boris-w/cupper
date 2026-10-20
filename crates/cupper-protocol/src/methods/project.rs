//! Projects and agents (design 5.3.4, 5.2.6, 5.6.7): adding and trusting a
//! project, and what the host knows about each agent.

use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Serialize};

use super::Empty;
use super::catalog::Catalog;
use crate::annotations::{TimestampMs, WirePath};
use crate::capabilities::SessionCapabilities;
use crate::domain::AgentKind;
use crate::ids::{CommandId, Digest, ProjectId};
use crate::index::{AgentStatus, TrustState};
use crate::item::DiffChange;
use crate::open_enum;

method! {
    /// `project/add`: adding the same path again returns the project it already is.
    request ProjectAdd = "project/add" (ProjectAddParams) -> ProjectAddResult {
        scope: Write, idempotency: Command, milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectAddParams {
    pub command_id: CommandId,
    /// The directory; the host resolves its realpath.
    pub path: WirePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectAddResult {
    pub project_id: ProjectId,
    pub trust: TrustScan,
}

/// A trust scan (design 5.2.6). A project with no configuration file in the
/// scan's range is `trusted` at once; otherwise it starts `untrusted`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TrustScan {
    pub state: TrustState,
    pub findings: Vec<Finding>,
    /// Findings left out to keep the result within its frame; the digest
    /// covers them all (T034).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings_omitted: Option<u32>,
    /// What `project/trust` echoes back.
    pub trust_digest: Digest,
}

/// Something in the project's configuration the agent would load, listed in
/// the native confirmation (design 5.2.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Finding {
    pub kind: FindingKind,
    /// The file it is in, relative to the project; the levels above it, up
    /// to the git root, start with `../`.
    pub path: WirePath,
    /// The realpath a symlink on `path` resolves to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<WirePath>,
    /// `target` lies outside the workspace; the dialog says so prominently.
    pub outside_workspace: bool,
    /// What it runs or sets, as the file writes it; shown verbatim.
    pub detail: String,
}

open_enum! {
    pub enum FindingKind {
        Hook => "hook",
        Env => "env",
        ApiKeyHelper => "api_key_helper",
        StatusLine => "status_line",
        McpServer => "mcp_server",
        Skill => "skill",
        Agent => "agent",
        Command => "command",
        /// A setting or hook in `.codex/`.
        CodexConfig => "codex_config",
        /// A file that could not be parsed; `detail` says why. Its whole
        /// content still counts in the digest.
        Unparsed => "unparsed",
    }
}

method! {
    /// `project/trust`: sent by the main process after the native
    /// confirmation. The host scans again and trusts an unchanged digest only.
    request ProjectTrust = "project/trust" (ProjectTrustParams) -> ProjectTrustResult {
        scope: Privileged, idempotency: Command, milestone: M1a,
        errors: [Conflict / DigestMismatch, Unavailable / ScanFailed],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectTrustParams {
    pub command_id: CommandId,
    pub project_id: ProjectId,
    /// The digest of the scan the user confirmed.
    pub trust_digest: Digest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectTrustResult {
    pub trust_state: TrustState,
}

method! {
    /// `project/trust_info`: scans again, for the native confirmation.
    request ProjectTrustInfo = "project/trust_info" (ProjectTrustInfoParams) -> ProjectTrustInfoResult {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [NotFound, Unavailable / ScanFailed],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectTrustInfoParams {
    pub project_id: ProjectId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectTrustInfoResult {
    pub state: TrustState,
    pub findings: Vec<Finding>,
    /// As [`TrustScan::findings_omitted`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub findings_omitted: Option<u32>,
    pub trust_digest: Digest,
    /// `stale` only: the files changed since the project was trusted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_from_trusted: Option<Vec<TrustedFileChange>>,
    /// Changes left out of `diff_from_trusted` for the frame's sake.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff_omitted: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TrustedFileChange {
    /// As in [`Finding::path`].
    pub path: WirePath,
    pub change: DiffChange,
}

method! {
    /// `project/remove`: archives the project's sessions too.
    request ProjectRemove = "project/remove" (ProjectRemoveParams) -> Empty {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [Conflict / TurnRunning],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRemoveParams {
    pub command_id: CommandId,
    pub project_id: ProjectId,
}

method! {
    /// `agents/capabilities`: the agent's latest capabilities from the host's
    /// cache, so the new-session page shows them before the first message.
    request AgentsCapabilities = "agents/capabilities" (AgentsCapabilitiesParams) -> AgentsCapabilitiesResult {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [Unavailable],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsCapabilitiesParams {
    pub agent: AgentKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsCapabilitiesResult {
    /// Absent while the cache is empty; `agents/refresh` fills it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached: Option<CachedCapabilities>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CachedCapabilities {
    pub capabilities: SessionCapabilities,
    /// When the host read them from the agent.
    pub captured_at: TimestampMs,
}

method! {
    /// `agents/refresh`: finds the CLI again and checks its version; probes
    /// the agent when the capability cache is empty (design 5.6.7).
    request AgentsRefresh = "agents/refresh" (AgentsRefreshParams) -> AgentsRefreshResult {
        scope: Write, idempotency: Natural, milestone: M1a,
        errors: [Unavailable],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsRefreshParams {
    pub agent: AgentKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsRefreshResult {
    pub status: AgentStatus,
}

method! {
    /// `agents/restart`: restarts the agent's adapter, which all its sessions share.
    request AgentsRestart = "agents/restart" (AgentsRestartParams) -> Empty {
        scope: Write, idempotency: Command, milestone: M1b,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsRestartParams {
    pub command_id: CommandId,
    pub agent: AgentKind,
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<ProjectAdd>(g);
    c.request::<ProjectTrust>(g);
    c.request::<ProjectTrustInfo>(g);
    c.request::<ProjectRemove>(g);
    c.request::<AgentsCapabilities>(g);
    c.request::<AgentsRefresh>(g);
    c.request::<AgentsRestart>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::methods::testkit::{capabilities, digest, request};
    use crate::testutil::open_values;

    /// Typical values of every method in this group (design 5.3.4, 5.2.6).
    pub fn samples() -> Vec<&'static str> {
        let findings = json!([
            {"kind": "hook", "path": ".claude/settings.json", "outside_workspace": false,
                "detail": "npm run lint"},
            {"kind": "mcp_server", "path": ".mcp.json", "target": "/work/dotfiles/mcp.json",
                "outside_workspace": true, "detail": "github: npx server-github"},
            {"kind": "codex_config", "path": "../.codex/config.toml", "outside_workspace": false,
                "detail": "sandbox_mode"},
        ]);
        vec![
            request::<ProjectAdd>(
                json!({"command_id": "c-1", "path": "/work/app"}),
                json!({"project_id": "p_1", "trust": {"state": "untrusted", "findings": findings,
                    "trust_digest": digest('a')}}),
            ),
            request::<ProjectAdd>(
                json!({"command_id": "c-2", "path": "/work/empty"}),
                json!({"project_id": "p_2", "trust": {"state": "trusted", "findings": [],
                    "trust_digest": digest('b')}}),
            ),
            request::<ProjectTrust>(
                json!({"command_id": "c-3", "project_id": "p_1", "trust_digest": digest('a')}),
                json!({"trust_state": "trusted"}),
            ),
            request::<ProjectTrustInfo>(
                json!({"project_id": "p_1"}),
                json!({"state": "stale", "findings": findings, "trust_digest": digest('c'),
                    "diff_from_trusted": [{"path": ".mcp.json", "change": "update"},
                        {"path": "../.codex/config.toml", "change": "add"}]}),
            ),
            request::<ProjectTrustInfo>(
                json!({"project_id": "p_1"}),
                json!({"state": "untrusted", "findings": [], "trust_digest": digest('a')}),
            ),
            request::<ProjectRemove>(json!({"command_id": "c-4", "project_id": "p_1"}), json!({})),
            request::<AgentsCapabilities>(
                json!({"agent": "claude"}),
                json!({"cached": {"capabilities": capabilities(), "captured_at": 1700000000000_u64}}),
            ),
            request::<AgentsCapabilities>(json!({"agent": "codex"}), json!({})),
            request::<AgentsRefresh>(
                json!({"agent": "codex"}),
                json!({"status": {"agent": "codex", "state": "idle",
                    "cli": {"path": "/opt/homebrew/bin/codex", "version": "0.50.0",
                        "compat": "verified", "install": "native"},
                    "adapter": {"version": "2.2.2", "runtime": "ready"},
                    "auth": {"state": "logged_in", "kind": "chatgpt"}, "config_dir": "default"}}),
            ),
            request::<AgentsRestart>(json!({"command_id": "c-5", "agent": "codex"}), json!({})),
        ]
    }

    #[test]
    fn unit_proto_methods_core_session_findings_are_open() {
        open_values::<FindingKind>(FindingKind::KNOWN);
    }
}
