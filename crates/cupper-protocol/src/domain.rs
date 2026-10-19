//! Domain types shared by events, methods and both streams (design 5.3.1,
//! 5.3.4, 5.3.11): content and prompts, who is acting, and failure codes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::annotations::WirePath;
use crate::ids::{BlobHash, PrincipalId};
use crate::{open_enum, open_union};

open_union! {
    /// Content inline or, when its escaped size exceeds the inline limit, a
    /// blob reference fetched with `blob/get` (design 5.3.1).
    pub enum Content in tag "type" {
        Text(TextContent) => "text",
        Blob(BlobContent) => "blob",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextContent {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobContent {
    pub blob: BlobRef,
    /// The start of the content, for display before the blob is fetched.
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobRef {
    pub hash: BlobHash,
    /// Size in bytes.
    pub size: u64,
    pub media_type: String,
}

/// What the user sends to the agent: a list of blocks (design 5.3.4).
pub type PromptContent = Vec<PromptBlock>;

open_union! {
    /// One block of [`PromptContent`].
    pub enum PromptBlock in tag "type" {
        Text(TextContent) => "text",
        /// A pasted image, uploaded with `blob/put` first.
        Image(BlobBlock) => "image",
        /// An `@` reference to a file.
        FileRef(FileRef) => "file_ref",
        /// An inline review comment; the host lays it out as text for the agent.
        ReviewComment(ReviewComment) => "review_comment",
        /// Text too large for one frame, uploaded with `blob/put` first.
        TextBlob(BlobBlock) => "text_blob",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlobBlock {
    pub blob: BlobRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FileRef {
    /// Relative to the workspace.
    pub path: WirePath,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReviewComment {
    /// Relative to the workspace.
    pub path: WirePath,
    /// First commented line, from 1, on `side`.
    pub line_start: u32,
    /// Last commented line, inclusive.
    pub line_end: u32,
    pub side: DiffSide,
    /// The commented lines as the user saw them.
    pub quote: String,
    pub comment: String,
}

open_enum! {
    /// Which version of a file a diff line number refers to.
    pub enum DiffSide {
        Old => "old",
        New => "new",
    }
}

open_enum! {
    pub enum AgentKind {
        Claude => "claude",
        Codex => "codex",
    }
}

open_enum! {
    /// The kind of principal behind a connection (design 5.8.2).
    pub enum PrincipalKind {
        DesktopUi => "desktop_ui",
        Cli => "cli",
        TestHarness => "test_harness",
        /// M3.
        RemoteDevice => "remote_device",
    }
}

open_enum! {
    /// What a client says it is in `initialize` (design 5.3.2).
    pub enum ClientKind {
        Desktop => "desktop",
        Cli => "cli",
        Ios => "ios",
        Test => "test",
    }
}

open_enum! {
    /// The role of a `desktop_ui` connection, fixed at `initialize` (design 5.7.1).
    pub enum ClientRole {
        /// The main process's own connection.
        Control => "control",
        /// Data forwarded to one window.
        Window => "window",
    }
}

open_enum! {
    /// Approval tier, ordered `normal` < `elevated` < `full_access` (design 5.6.5).
    pub enum Tier {
        /// The user approves each out-of-bounds action.
        Normal => "normal",
        /// A classifier or rule approves some actions for the user.
        Elevated => "elevated",
        /// Nothing is approved.
        FullAccess => "full_access",
    }
}

open_enum! {
    /// Why a turn, an agent or a session failed (design 5.3.11): the agent
    /// error classes of design 5.6.2 plus host-side causes.
    pub enum FailureCode {
        NotLoggedIn => "not_logged_in",
        CliTooOld => "cli_too_old",
        SessionNotFound => "session_not_found",
        ProcessExited => "process_exited",
        /// The agent restarts its backend on the next prompt.
        Restartable => "restartable",
        ModelUnsupported => "model_unsupported",
        QuotaExhausted => "quota_exhausted",
        /// An agent error that fits no other class.
        Other => "other",
        AdapterExited => "adapter_exited",
        /// Restarted too often; needs a manual restart in settings.
        AdapterCrashed => "adapter_crashed",
        CliMissing => "cli_missing",
        RuntimeFailed => "runtime_failed",
        HostRestarted => "host_restarted",
    }
}

/// Who resolved an interaction request (design 5.3.5), for "handled
/// elsewhere" hints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ResolvedBy {
    pub principal_id: PrincipalId,
    pub client_kind: ClientKind,
    pub client_name: String,
}

/// Money spent, as the agent reports it (ACP `usage_update`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cost {
    pub amount: f64,
    /// ISO 4217 code, e.g. `USD`.
    pub currency: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::testutil::{open_values, roundtrip};

    #[test]
    fn unit_proto_events_index_content_and_prompt_roundtrip() {
        let blob = json!({"hash": "ab".repeat(32), "size": 70000, "media_type": "text/plain"});
        let content: Vec<Content> = roundtrip(&json!([
            {"type": "text", "text": "ok"},
            {"type": "blob", "blob": blob, "preview": "line 1"},
            {"type": "diagram", "source": "a -> b"},
        ]));
        let tags: Vec<&str> = content.iter().map(Content::tag).collect();
        assert_eq!(tags, ["text", "blob", "diagram"]);
        assert!(matches!(content[2], Content::Unknown(_)));

        let prompt: PromptContent = roundtrip(&json!([
            {"type": "text", "text": "fix it"},
            {"type": "image", "blob": {"hash": "cd".repeat(32), "size": 9, "media_type": "image/png"}},
            {"type": "file_ref", "path": "src/a.rs"},
            {"type": "review_comment", "path": "src/a.rs", "line_start": 3, "line_end": 4,
             "side": "new", "quote": "let x = 1;", "comment": "rename"},
            {"type": "text_blob", "blob": blob},
            {"type": "review_comment", "path": "b.rs", "line_start": 1, "line_end": 1,
             "side": "both", "quote": "", "comment": "unknown side"},
            {"type": "voice", "blob": blob},
        ]));
        let tags: Vec<&str> = prompt.iter().map(PromptBlock::tag).collect();
        assert_eq!(tags[..5], *PromptBlock::TAGS);
        assert_eq!(tags[5..], ["review_comment", "voice"]);
        let PromptBlock::ReviewComment(comment) = &prompt[5] else {
            panic!()
        };
        assert_eq!(comment.side, DiffSide::Unknown("both".into()));
    }

    #[test]
    fn unit_proto_events_index_domain_rejects_malformed_members() {
        for bad in [
            json!({"type": "blob", "blob": {"hash": "x y", "size": 1, "media_type": "a/b"}, "preview": ""}),
            json!({"type": "blob", "preview": ""}),
            json!({"text": "no tag"}),
        ] {
            assert!(
                serde_json::from_value::<Content>(bad.clone()).is_err(),
                "{bad}"
            );
        }
        let bad = json!({"type": "review_comment", "path": "a.rs", "line_start": 1, "side": "old",
            "quote": "", "comment": ""});
        let err = serde_json::from_value::<PromptBlock>(bad).unwrap_err();
        assert!(
            err.to_string().contains("missing field `line_end`"),
            "{err}"
        );
    }

    #[test]
    fn unit_proto_events_index_domain_enums_and_records_roundtrip() {
        let by: ResolvedBy = roundtrip(&json!(
            {"principal_id": "pr_cli", "client_kind": "watch", "client_name": "cupper-cli"}));
        assert_eq!(by.client_kind, ClientKind::Unknown("watch".into()));
        roundtrip::<Cost>(&json!({"amount": 0.125, "currency": "USD"}));
        open_values::<DiffSide>(DiffSide::KNOWN);
        open_values::<AgentKind>(AgentKind::KNOWN);
        open_values::<PrincipalKind>(PrincipalKind::KNOWN);
        open_values::<ClientKind>(ClientKind::KNOWN);
        open_values::<ClientRole>(ClientRole::KNOWN);
        open_values::<Tier>(Tier::KNOWN);
        open_values::<FailureCode>(FailureCode::KNOWN);
    }
}
