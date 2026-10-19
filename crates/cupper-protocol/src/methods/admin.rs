//! Settings, authorization, diagnostics and tests (design 5.3.4, 5.2.11,
//! 5.8.4, 5.2.16, 5.9.9): the host's settings, CLI elevation, what a
//! privileged command would do, diagnostics exports, and the test-harness
//! methods.

use schemars::{JsonSchema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

use super::Empty;
use super::catalog::Catalog;
use crate::annotations::{DurationMs, TimestampMs, WirePath};
use crate::domain::{AgentKind, BlobRef};
use crate::ids::{CommandId, ElevationId, SessionId};
use crate::open_enum;
use crate::rpc::Scope;

method! {
    /// `settings/get`.
    request SettingsGet = "settings/get" (Empty) -> SettingsResult {
        scope: Read,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
    }
}

method! {
    /// `settings/update`: change some keys; returns all of them.
    request SettingsUpdate = "settings/update" (SettingsUpdateParams) -> SettingsResult {
        scope: Write,
        idempotency: Command,
        milestone: M1a,
        errors: [Forbidden],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SettingsUpdateParams {
    pub command_id: CommandId,
    pub patch: SettingsPatch,
}

/// The result of `settings/get` and `settings/update`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SettingsResult {
    pub settings: Settings,
}

/// Declares [`Settings`] and [`SettingsPatch`] from one list of keys.
macro_rules! settings {
    ($($(#[doc = $doc:literal])* $key:literal => $field:ident: $ty:ty,)+) => {
        /// The host's settings by their keys in design 5.2.11, every key
        /// present. Changing `agents.*.cli_path`, `agents.env_passthrough`
        /// or `diagnostics.verbose` takes `privileged` (design 5.2.5).
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub struct Settings {
            $($(#[doc = $doc])* #[serde(rename = $key)] pub $field: $ty,)+
        }

        /// The keys to change, each with its new value; `null` clears a
        /// key whose value may be `null`.
        #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
        pub struct SettingsPatch {
            $(
                #[serde(rename = $key, default, deserialize_with = "present")]
                #[serde(skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )+
        }
    };
}

settings! {
    /// `null`: the CLI is discovered (design 5.6.6).
    "agents.claude.cli_path" => claude_cli_path: Option<WirePath>,
    /// `null`: the CLI is discovered.
    "agents.codex.cli_path" => codex_cli_path: Option<WirePath>,
    /// Names of more environment variables the adapters get (design 5.6.6).
    "agents.env_passthrough" => env_passthrough: Vec<String>,
    "worktrees.root" => worktrees_root: WirePath,
    "notifications.needs_action" => notify_needs_action: bool,
    "notifications.turn_completed" => notify_turn_completed: bool,
    "notifications.error" => notify_error: bool,
    /// Apps of the host's built-in list, for `workspace/open`.
    "apps.terminal" => terminal_app: String,
    "apps.editor" => editor_app: String,
    "sessions.default_agent" => default_agent: AgentKind,
    "host.idle_exit_minutes" => idle_exit_minutes: u32,
    /// Detailed logs, which may contain message text.
    "diagnostics.verbose" => verbose_diagnostics: bool,
}

/// A key in the patch is `Some`, even with `null`.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(d: D) -> Result<Option<T>, D::Error> {
    T::deserialize(d).map(Some)
}

method! {
    /// `auth/elevate`: a CLI asks the user for more scopes and gets the
    /// answer once the user decides, within 2 minutes (design 5.8.4). The
    /// policy refuses every principal but `cli`.
    request AuthElevate = "auth/elevate" (AuthElevateParams) -> AuthElevateResult {
        scope: None,
        idempotency: None,
        milestone: M1a,
        errors: [Forbidden / Lineage, RateLimited, Timeout],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthElevateParams {
    /// `write`, `approve` or both.
    pub scopes: Vec<Scope>,
    /// Shown as unverified.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthElevateResult {
    /// What the user granted, maybe less than asked for; empty when denied.
    pub granted: Vec<Scope>,
    /// Absent when nothing was granted. The grant also ends with the connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<TimestampMs>,
}

method! {
    /// `auth/elevation_get`: what the native dialog shows about an elevation request.
    request AuthElevationGet = "auth/elevation_get" (AuthElevationGetParams) -> AuthElevationGetResult {
        scope: Privileged,
        idempotency: Natural,
        milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthElevationGetParams {
    pub elevation_id: ElevationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthElevationGetResult {
    pub verified: VerifiedRequester,
    pub self_reported: SelfReported,
    pub scopes: Vec<Scope>,
}

/// What the host checked about the requesting process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VerifiedRequester {
    pub executable: WirePath,
    /// The signing identity; absent when unsigned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signing: Option<String>,
    pub pid: u32,
    /// From the parent up.
    pub parent_chain: Vec<ChainProcess>,
    pub has_tty: bool,
    /// The chain ends at launchd, or there is no controlling terminal.
    pub origin_unverifiable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChainProcess {
    pub pid: u32,
    pub executable: WirePath,
}

/// What the requester says, unverified.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SelfReported {
    pub client_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

method! {
    /// `auth/elevation_respond`: the user's decision in the native dialog.
    request AuthElevationRespond = "auth/elevation_respond" (AuthElevationRespondParams) -> Empty {
        scope: Privileged,
        idempotency: Command,
        milestone: M1a,
        errors: [Expired],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthElevationRespondParams {
    pub command_id: CommandId,
    pub elevation_id: ElevationId,
    pub decision: ElevationAnswer,
    /// What to grant, of what was asked for.
    pub scopes: Vec<Scope>,
}

open_enum! {
    pub enum ElevationAnswer {
        Grant => "grant",
        Deny => "deny",
    }
}

method! {
    /// `policy/describe`: what a command would do, in semantic codes, for the
    /// native dialog the main process shows before it sends the command.
    request PolicyDescribe = "policy/describe" (PolicyDescribeParams) -> PolicyDescribeResult {
        scope: Privileged,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
    }
}

/// A command as the main process will send it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyDescribeParams {
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyDescribeResult {
    pub action: ConfirmAction,
    /// The action's parameters, e.g. a setting's key and value or an
    /// option's effect; codes and values, no UI text.
    pub details: Map<String, Value>,
}

open_enum! {
    /// What the user confirms: one value per "原生确认" of design 5.8.3.
    pub enum ConfirmAction {
        /// `project/trust`.
        TrustProject => "trust_project",
        /// `session/confirm_config`.
        ConfirmConfig => "confirm_config",
        /// To `full_access` by `session/create`, `session/set_config` or an option's `mode_after`.
        FullAccess => "full_access",
        /// `interaction/respond` with a `requires: privileged` option.
        PrivilegedOption => "privileged_option",
        /// `settings/update` of a security-related key.
        SecuritySetting => "security_setting",
        /// `auth/elevation_respond` with `grant`.
        GrantElevation => "grant_elevation",
        /// `host/exit` or `host/prepare_upgrade` while turns run.
        InterruptTurns => "interrupt_turns",
    }
}

method! {
    /// `diag/export`: a diagnostics zip (design 5.11.3), which the main process saves.
    request DiagExport = "diag/export" (DiagExportParams) -> DiagExportResult {
        scope: UiLocal,
        idempotency: Command,
        milestone: M2,
        errors: [],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiagExportParams {
    pub command_id: CommandId,
    /// Sessions whose raw frames to add, as the user picked them; none by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_sessions: Vec<SessionId>,
}

/// The result of `diag/export` and `diag/recording_export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiagExportResult {
    /// Fetched with `blob/get`.
    pub blob: BlobRef,
}

method! {
    /// `diag/recording_export`: a session's raw frames as a recording (design 5.9.3).
    request DiagRecordingExport = "diag/recording_export" (DiagRecordingExportParams) -> DiagExportResult {
        scope: UiLocal,
        idempotency: Natural,
        milestone: M1a,
        errors: [],
        builds: [TestHarness, DevAuth],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DiagRecordingExportParams {
    pub session_id: SessionId,
}

method! {
    /// `test/clock_advance`: advance the manual clock; returns once every
    /// timer due has fired and what it caused is done (design 5.2.16).
    request TestClockAdvance = "test/clock_advance" (TestClockAdvanceParams) -> Empty {
        scope: Test,
        idempotency: None,
        milestone: M1a,
        errors: [],
        builds: [TestHarness],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TestClockAdvanceParams {
    pub ms: DurationMs,
}

method! {
    /// `test/run_maintenance`: run a background task now; returns when it is done.
    request TestRunMaintenance = "test/run_maintenance" (TestRunMaintenanceParams) -> Empty {
        scope: Test,
        idempotency: None,
        milestone: M1a,
        errors: [],
        builds: [TestHarness],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TestRunMaintenanceParams {
    pub task: MaintenanceTask,
}

open_enum! {
    /// A cleanup the host runs on its own schedule (design 5.4.5).
    pub enum MaintenanceTask {
        /// Command records older than 48 hours.
        Ledger => "ledger",
        /// Raw frames older than 14 days or beyond 512 MB.
        RawFrames => "raw_frames",
        /// Audit records older than 90 days.
        Audit => "audit",
        /// Unreferenced blobs older than a day.
        OrphanBlobs => "orphan_blobs",
    }
}

method! {
    /// `test/seed`: synthetic sessions through the normal write path, for
    /// cold start, long sessions and SP-6 (design 5.9.9).
    request TestSeed = "test/seed" (TestSeedParams) -> Empty {
        scope: Test,
        idempotency: None,
        milestone: M1a,
        errors: [],
        builds: [TestHarness],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TestSeedParams {
    pub profile: SeedProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SeedProfile {
    pub sessions: u32,
    /// Ended turns per session.
    pub turns: u32,
    pub items_per_turn: u32,
    /// The kinds of items and the lengths of their text.
    pub mix: SeedMix,
    /// One more turn in the first session with this many items (SP-6: 2,000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large_turn_items: Option<u32>,
}

open_enum! {
    pub enum SeedMix {
        /// As in the SP-1 recordings.
        Sp1 => "sp1",
    }
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<SettingsGet>(g);
    c.request::<SettingsUpdate>(g);
    c.request::<AuthElevate>(g);
    c.request::<AuthElevationGet>(g);
    c.request::<AuthElevationRespond>(g);
    c.request::<PolicyDescribe>(g);
    c.request::<DiagExport>(g);
    c.request::<DiagRecordingExport>(g);
    c.request::<TestClockAdvance>(g);
    c.request::<TestRunMaintenance>(g);
    c.request::<TestSeed>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::methods::testkit::request;
    use crate::testutil::open_values;

    /// Typical values of every method in this group (design 5.3.4).
    pub fn samples() -> Vec<&'static str> {
        let settings = json!({"settings": {"agents.claude.cli_path": "/opt/homebrew/bin/claude",
            "agents.codex.cli_path": null, "agents.env_passthrough": ["ANTHROPIC_BASE_URL"],
            "worktrees.root": "~/.cupper/worktrees", "notifications.needs_action": true,
            "notifications.turn_completed": true, "notifications.error": true,
            "apps.terminal": "terminal", "apps.editor": "vscode", "sessions.default_agent": "claude",
            "host.idle_exit_minutes": 30, "diagnostics.verbose": false}});
        let blob = |media_type| json!({"blob": {"hash": "ab".repeat(32), "size": 9000, "media_type": media_type}});
        vec![
            request::<SettingsGet>(json!({}), settings.clone()),
            request::<SettingsUpdate>(
                json!({"command_id": "c-1", "patch": {"notifications.error": false,
                    "agents.claude.cli_path": null}}),
                settings,
            ),
            request::<AuthElevate>(
                json!({"scopes": ["write", "approve"], "reason": "apply the review"}),
                json!({"granted": ["write"], "expires_at": 1700000900000_u64}),
            ),
            request::<AuthElevate>(json!({"scopes": ["write"]}), json!({"granted": []})),
            request::<AuthElevationGet>(
                json!({"elevation_id": "e_1"}),
                json!({"verified": {"executable": "/usr/local/bin/cupper", "signing": "Developer ID",
                    "pid": 4242, "parent_chain": [{"pid": 4100, "executable": "/bin/zsh"}],
                    "has_tty": true, "origin_unverifiable": false},
                    "self_reported": {"client_name": "cupper-cli", "reason": "apply the review"},
                    "scopes": ["write", "approve"]}),
            ),
            request::<AuthElevationRespond>(
                json!({"command_id": "c-2", "elevation_id": "e_1", "decision": "grant", "scopes": ["write"]}),
                json!({}),
            ),
            request::<PolicyDescribe>(
                json!({"method": "settings/update", "params": {"command_id": "c-1",
                    "patch": {"agents.env_passthrough": ["OPENAI_API_KEY"]}}}),
                json!({"action": "security_setting", "details": {"key": "agents.env_passthrough",
                    "value": ["OPENAI_API_KEY"]}}),
            ),
            request::<DiagExport>(
                json!({"command_id": "c-3", "include_sessions": ["s_1"]}),
                blob("application/zip"),
            ),
            request::<DiagRecordingExport>(json!({"session_id": "s_1"}), blob("application/jsonl")),
            request::<TestClockAdvance>(json!({"ms": 3000}), json!({})),
            request::<TestRunMaintenance>(json!({"task": "orphan_blobs"}), json!({})),
            request::<TestSeed>(
                json!({"profile": {"sessions": 200, "turns": 20, "items_per_turn": 50,
                    "mix": "sp1", "large_turn_items": 2000}}),
                json!({}),
            ),
        ]
    }

    #[test]
    fn unit_proto_methods_ext_settings_patch_sets_and_clears() {
        let patch = |raw| serde_json::from_value::<SettingsPatch>(raw);
        let set = patch(json!({"agents.codex.cli_path": "/usr/local/bin/codex"})).unwrap();
        assert_eq!(
            set.codex_cli_path,
            Some(Some(WirePath("/usr/local/bin/codex".into())))
        );
        let cleared = patch(json!({"agents.codex.cli_path": null})).unwrap();
        assert_eq!(cleared.codex_cli_path, Some(None));
        assert_eq!(cleared.claude_cli_path, None);
        assert_eq!(patch(json!({})).unwrap(), SettingsPatch::default());
        // Only a key whose value may be `null` can be cleared.
        assert!(patch(json!({"notifications.error": null})).is_err());

        open_values::<ElevationAnswer>(ElevationAnswer::KNOWN);
        open_values::<ConfirmAction>(ConfirmAction::KNOWN);
        open_values::<MaintenanceTask>(MaintenanceTask::KNOWN);
        open_values::<SeedMix>(SeedMix::KNOWN);
    }
}
