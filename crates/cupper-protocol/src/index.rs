//! Data of the index stream's events (design 5.3.5): projects, session
//! summaries, agents, the host, connected clients, elevation requests and
//! notification intents. `IndexActor` is the stream's only writer (design 5.2.9).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotations::{TimestampMs, WirePath};
use crate::capabilities::CliCompat;
use crate::domain::{AgentKind, ClientRole, FailureCode, PrincipalKind};
use crate::ids::{ConnId, ElevationId, IntentId, ProjectId, RequestId, SessionId};
use crate::open_enum;
use crate::rpc::Scope;

/// `project.upserted`: added, renamed, reordered, or its trust state changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectUpserted {
    pub project_id: ProjectId,
    pub name: String,
    /// Realpath of the project directory.
    pub path: WirePath,
    pub is_git: bool,
    pub added_at: TimestampMs,
    /// Sidebar order, ascending.
    pub position: u32,
    pub trust_state: TrustState,
}

open_enum! {
    /// Whether the project's own agent configuration may load (design 5.2.6).
    pub enum TrustState {
        Untrusted => "untrusted",
        Trusted => "trusted",
        /// Trusted once, but the configuration changed since; treated as untrusted.
        Stale => "stale",
    }
}

/// `project.removed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProjectRemoved {
    pub project_id: ProjectId,
}

/// `session.upserted`: one session in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionSummary {
    pub session_id: SessionId,
    pub project_id: ProjectId,
    pub agent: AgentKind,
    /// Empty while `title_source` is `none`.
    pub title: String,
    pub title_source: TitleSource,
    pub workspace_kind: WorkspaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub status: SessionStatus,
    pub unread: bool,
    /// Number of pending interaction requests.
    pub pending_interactions: u32,
    /// The class of the oldest pending request, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_kind: Option<PendingKind>,
    pub last_activity_at: TimestampMs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_code: Option<FailureCode>,
    /// A restored session whose branch no longer exists (PS-3).
    pub read_only: bool,
}

open_enum! {
    pub enum TitleSource {
        None => "none",
        /// Cut from the first message.
        Prompt => "prompt",
        Agent => "agent",
        /// Set by the user; the agent no longer overrides it.
        User => "user",
    }
}

open_enum! {
    pub enum WorkspaceKind {
        Local => "local",
        Worktree => "worktree",
    }
}

open_enum! {
    /// Attention state, first match wins in this order (design 5.2.9).
    pub enum SessionStatus {
        NeedsAction => "needs_action",
        Running => "running",
        Error => "error",
        /// A turn ended after the last mark-read.
        Unread => "unread",
        Idle => "idle",
    }
}

open_enum! {
    /// What the oldest pending request waits for (design 5.2.9, v1.4).
    pub enum PendingKind {
        /// `permission` or `plan_confirm`.
        Approval => "approval",
        /// `question` or `auth_url`.
        Input => "input",
    }
}

/// `session.removed`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SessionRemoved {
    pub session_id: SessionId,
    pub reason: RemovalReason,
}

open_enum! {
    pub enum RemovalReason {
        Archived => "archived",
    }
}

/// `agents.updated`: detection results and resource samples.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentsUpdated {
    pub agents: Vec<AgentStatus>,
    pub git: GitTools,
}

/// The git tools found on the login shell's PATH.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GitTools {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// The git-lfs version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lfs: Option<String>,
}

/// One agent as the settings page and the new-session page show it (AG-2, AG-5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgentStatus {
    pub agent: AgentKind,
    pub state: AgentState,
    /// Absent when no CLI was found.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli: Option<CliStatus>,
    pub adapter: AdapterStatus,
    pub auth: AuthStatus,
    /// Resident memory of the adapter's process group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rss_bytes: Option<u64>,
    pub config_dir: ConfigDirState,
}

open_enum! {
    pub enum AgentState {
        /// Cannot run: see `cli`, `adapter` and `auth` for why.
        Unavailable => "unavailable",
        /// Ready; the adapter is not running.
        Idle => "idle",
        Starting => "starting",
        Running => "running",
        /// Failed too often; restarts only by hand (design 5.6.8).
        Crashed => "crashed",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CliStatus {
    /// The resolved native binary.
    pub path: WirePath,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub compat: CliCompat,
    pub install: CliInstall,
}

open_enum! {
    /// How the CLI was resolved to a native binary (design 5.6.6).
    pub enum CliInstall {
        Native => "native",
        /// An npm shim, resolved to the package's platform binary.
        NpmShim => "npm_shim",
        /// Only a JavaScript entry point: not usable.
        UnsupportedInstall => "unsupported_install",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AdapterStatus {
    pub version: String,
    pub runtime: RuntimeState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<RuntimeFailure>,
}

open_enum! {
    /// The adapter's runtime installation (design 5.2.12).
    pub enum RuntimeState {
        Missing => "missing",
        Installing => "installing",
        Ready => "ready",
        Failed => "failed",
    }
}

open_enum! {
    pub enum RuntimeFailure {
        Network => "network",
        HashMismatch => "hash_mismatch",
        Disk => "disk",
        BundleChanged => "bundle_changed",
    }
}

/// Login state: the kind and the plan name only, never an email or organization.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthStatus {
    pub state: AuthState,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<String>,
}

open_enum! {
    pub enum AuthState {
        LoggedIn => "logged_in",
        LoggedOut => "logged_out",
        /// Not known until the agent reports it or a prompt fails.
        Unchecked => "unchecked",
    }
}

open_enum! {
    /// The agent's configuration directory from the login shell
    /// (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`; design 5.6.6).
    pub enum ConfigDirState {
        /// Not set; the agent uses its default.
        Default => "default",
        Ok => "ok",
        /// Set, but failed the checks and was not passed on.
        Rejected => "rejected",
    }
}

/// `host.updated`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HostUpdated {
    pub version: String,
    /// The host runs with degraded local authentication (ADR-05).
    pub weak_auth: bool,
    pub pending_upgrade: bool,
    /// The host keeps the Mac awake while turns run.
    pub power_assertion: bool,
    /// The previous run did not exit cleanly; present only when true.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub last_exit_abnormal: bool,
}

/// `clients.updated`: on connect, disconnect and elevation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientsUpdated {
    pub clients: Vec<ClientInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ClientInfo {
    pub conn_id: ConnId,
    pub principal_kind: PrincipalKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<ClientRole>,
    pub client_name: String,
    pub client_version: String,
    pub since: TimestampMs,
    pub auth: ClientAuth,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub elevated: Option<Elevation>,
}

open_enum! {
    pub enum ClientAuth {
        /// Code signature verified.
        Verified => "verified",
        /// Degraded authentication (ADR-05).
        Weak => "weak",
        /// Same user ID only.
        Peer => "peer",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Elevation {
    pub scopes: Vec<Scope>,
    pub expires_at: TimestampMs,
}

/// `elevation.requested`: a CLI asks for more scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ElevationRequested {
    pub elevation_id: ElevationId,
    /// As the CLI reports it; not verified.
    pub client_name: String,
    pub scopes: Vec<Scope>,
    pub requested_at: TimestampMs,
}

/// `elevation.resolved`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ElevationResolved {
    pub elevation_id: ElevationId,
    pub decision: ElevationDecision,
}

open_enum! {
    pub enum ElevationDecision {
        Granted => "granted",
        Denied => "denied",
        Expired => "expired",
    }
}

/// `notification.intent`: written when no foreground client watches the
/// session; never part of a snapshot (design 5.2.9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationIntent {
    pub intent_id: IntentId,
    pub session_id: SessionId,
    pub reason: IntentReason,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
}

open_enum! {
    pub enum IntentReason {
        NeedsAction => "needs_action",
        TurnCompleted => "turn_completed",
        Error => "error",
    }
}
