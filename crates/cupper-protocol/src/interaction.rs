//! Interaction requests (design 5.3.6): what the agent asks, the options
//! with their effects, the answers, and the digest a client echoes back.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Number;

use crate::annotations::{TimestampMs, WirePath};
use crate::canonical::{self, CanonicalError};
use crate::domain::{Content, PrincipalKind, ResolvedBy, Tier};
use crate::ids::{Digest, ItemId, RequestId, SessionId, TurnId};
use crate::item::ToolKind;
use crate::rpc::Scope;
use crate::{open_enum, open_union};

/// `interaction.requested`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRequested {
    pub request_id: RequestId,
    /// Absent for a request outside any turn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<TurnId>,
    /// The tool call the request is about.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<ItemId>,
    pub created_at: TimestampMs,
    /// [`request_digest`], echoed by `interaction/respond`.
    pub digest: Digest,
    /// Who may answer; the host checks again on `interaction/respond`.
    pub actionable_by: Vec<PrincipalKind>,
    pub request: InteractionRequest,
}

open_union! {
    /// What the agent asks. A new type needs a client feature first (design 5.3.2).
    pub enum InteractionRequest in tag "type" {
        Permission(PermissionRequest) => "permission",
        Question(QuestionRequest) => "question",
        PlanConfirm(PlanConfirmRequest) => "plan_confirm",
        /// Not produced in M1.
        AuthUrl(AuthUrlRequest) => "auth_url",
    }
}

/// What a tool wants to do. The content the user approves (`tool.command`,
/// `tool.paths`, `detail`) is never cut to the label limit (design 5.3.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PermissionRequest {
    pub title: String,
    pub tool: PermissionTool,
    pub options: Vec<InteractionOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Content>,
    /// Some content is a blob beyond the inline limit: approval stays
    /// disabled until the user has expanded it in full.
    pub content_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PermissionTool {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<ItemId>,
    pub tool_kind: ToolKind,
    /// The full command: inline, or a blob beyond the inline limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<Content>,
    pub paths: Vec<WirePath>,
}

/// One option of a permission request or a plan confirmation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionOption {
    pub option_id: String,
    /// The agent's words, verbatim and never cut.
    pub name: String,
    /// ACP's kind as the agent sent it, unknown values included.
    pub kind: OptionKind,
    pub effect: OptionEffect,
    /// `approve` or `privileged`: the scope choosing this option needs.
    pub requires: Scope,
}

open_enum! {
    pub enum OptionKind {
        AllowOnce => "allow_once",
        AllowAlways => "allow_always",
        RejectOnce => "reject_once",
        RejectAlways => "reject_always",
    }
}

/// What choosing an option does, from the driver's option table (design 5.6.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct OptionEffect {
    pub scope: EffectScope,
    /// Writes the agent's own permission rules or exec policy (AP-2).
    pub persistent: bool,
    /// Whether choosing it ends the whole turn; absent when not known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_turn: Option<bool>,
    /// The mode the session switches to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode_after: Option<String>,
    /// The tier of `mode_after`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier_after: Option<Tier>,
    /// The rule a persistent option writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<EffectRule>,
}

open_enum! {
    pub enum EffectScope {
        Once => "once",
        Session => "session",
        Persistent => "persistent",
        ModeSwitch => "mode_switch",
        Reject => "reject",
        PlanAccept => "plan_accept",
        PlanRevise => "plan_revise",
        /// The option's kind is unknown: persistent, privileged, desktop only.
        Unclassified => "unknown",
    }
}

/// Without `text` or `location`, the native confirmation says the agent
/// decides and its words are what counts (design 5.3.6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EffectRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Where the agent writes the rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// The command prefix the rule allows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    pub prefix_risk: PrefixRisk,
}

open_enum! {
    pub enum PrefixRisk {
        Normal => "normal",
        /// `bash`, `python*`, `env`, `sudo` and the like: as good as any command.
        Interpreter => "interpreter",
        /// No prefix could be parsed: may allow more than it seems.
        Unparsed => "unknown",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionRequest {
    pub prompt: String,
    pub fields: Vec<QuestionField>,
    /// The agent's form cannot be represented: the client offers only "skip".
    pub unsupported_schema: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionField {
    pub field_id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub field_type: FieldType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<Choice>>,
    /// Free text is accepted besides the choices.
    pub allow_custom: bool,
    pub required: bool,
}

open_enum! {
    pub enum FieldType {
        SingleChoice => "single_choice",
        MultiChoice => "multi_choice",
        Text => "text",
        Boolean => "boolean",
        Number => "number",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Choice {
    pub value: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanConfirmRequest {
    pub plan: Content,
    pub options: Vec<InteractionOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuthUrlRequest {
    pub url: String,
    pub message: String,
}

/// `interaction.resolved`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionResolved {
    pub request_id: RequestId,
    pub status: InteractionStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<InteractionOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<ResolvedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ResolutionReason>,
}

open_enum! {
    pub enum InteractionStatus {
        Pending => "pending",
        Resolved => "resolved",
        /// Withdrawn by the agent, or its turn was cancelled.
        Cancelled => "cancelled",
        /// Its adapter instance exited or the host restarted (SEC-10).
        Expired => "expired",
    }
}

open_enum! {
    /// Why a request was `cancelled` or `expired`.
    pub enum ResolutionReason {
        Withdrawn => "withdrawn",
        TurnCancelled => "turn_cancelled",
        AdapterExited => "adapter_exited",
        HostRestarted => "host_restarted",
        /// Still pending when its turn ended: the prompt returned or failed
        /// without waiting for the answer (design 5.3.11).
        TurnEnded => "turn_ended",
    }
}

open_union! {
    /// An answer to a request, as `interaction/respond` sends it.
    pub enum InteractionOutcome in tag "type" {
        /// To a `permission` or a `plan_confirm`.
        Selected(Selected) => "selected",
        /// To a `question`.
        Answer(Answer) => "answer",
        /// To any type, without an answer.
        Dismissed => "dismissed",
        /// To an `auth_url`.
        Opened => "opened",
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Selected {
    pub option_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Answer {
    /// Keyed by `field_id`.
    pub values: BTreeMap<String, AnswerValue>,
}

/// One answered field. A number keeps its JSON form, so an integer does not
/// come back as a float.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AnswerValue {
    Boolean(bool),
    Number(Number),
    Text(String),
    Choices(Vec<String>),
}

/// The digest of a request (design 5.3.6): `d_` + the sha-256 of the
/// RFC 8785 form of `{session_id, request_id, turn_id, request}`. An absent
/// `turn_id` is left out, as on the wire; a blob counts by its reference.
pub fn request_digest(
    session_id: &SessionId,
    request_id: &RequestId,
    turn_id: Option<&TurnId>,
    request: &InteractionRequest,
) -> Result<Digest, CanonicalError> {
    #[derive(Serialize)]
    struct Digested<'a> {
        session_id: &'a SessionId,
        request_id: &'a RequestId,
        #[serde(skip_serializing_if = "Option::is_none")]
        turn_id: Option<&'a TurnId>,
        request: &'a InteractionRequest,
    }
    canonical::digest(&Digested {
        session_id,
        request_id,
        turn_id,
        request,
    })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use sha2::{Digest as _, Sha256};

    use super::*;
    use crate::testutil::{open_values, roundtrip};

    fn sha(canonical: &str) -> Digest {
        Digest::from_sha256(&Sha256::digest(canonical.as_bytes()).into())
    }

    #[test]
    fn unit_proto_events_session_request_digest() {
        let (s, r, t) = (
            "s_1".parse().unwrap(),
            "r_1".parse().unwrap(),
            "t_1".parse().unwrap(),
        );
        let request: InteractionRequest = roundtrip(&json!({"type": "auth_url",
            "url": "https://example.com/a", "message": "Sign in ✓"}));
        let body = r#"{"message":"Sign in ✓","type":"auth_url","url":"https://example.com/a"}"#;
        assert_eq!(
            request_digest(&s, &r, Some(&t), &request).unwrap(),
            sha(&format!(
                r#"{{"request":{body},"request_id":"r_1","session_id":"s_1","turn_id":"t_1"}}"#
            ))
        );
        assert_eq!(
            request_digest(&s, &r, None, &request).unwrap(),
            sha(&format!(
                r#"{{"request":{body},"request_id":"r_1","session_id":"s_1"}}"#
            ))
        );

        // Any change to what the user approves changes the digest.
        let option = json!({"option_id": "allow-once", "name": "Allow", "kind": "allow_once",
            "effect": {"scope": "once", "persistent": false}, "requires": "approve"});
        let permission = json!({"type": "permission", "title": "Run", "options": [option],
            "tool": {"tool_kind": "execute", "command": {"type": "text", "text": "ls"}, "paths": []},
            "content_truncated": false});
        let digest =
            |raw: &serde_json::Value| request_digest(&s, &r, Some(&t), &roundtrip(raw)).unwrap();
        let mut other = permission.clone();
        other["tool"]["command"]["text"] = json!("ls -a");
        assert_ne!(digest(&permission), digest(&other));
        assert_ne!(
            digest(&permission),
            request_digest(&s, &r, None, &roundtrip(&permission)).unwrap()
        );
    }

    #[test]
    fn unit_proto_events_session_outcomes_and_answers() {
        let answer = json!({"type": "answer", "values": {"a": "x", "b": 3, "c": true,
            "d": ["p", "q"], "e": 2.5, "f": -9007199254740991_i64}});
        let InteractionOutcome::Answer(parsed) = roundtrip(&answer) else {
            panic!()
        };
        assert_eq!(parsed.values["b"], AnswerValue::Number(3.into()));
        assert_eq!(serde_json::to_string(&parsed.values["b"]).unwrap(), "3");
        let later: InteractionOutcome = roundtrip(&json!({"type": "later", "at": 5}));
        assert_eq!(later.tag(), "later");

        for bad in [
            json!({"type": "selected"}),
            json!({"type": "answer", "values": {"a": {"nested": 1}}}),
            json!({"type": "answer", "values": {"a": null}}),
        ] {
            assert!(
                serde_json::from_value::<InteractionOutcome>(bad.clone()).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn unit_proto_events_session_resolved_turn_ended_roundtrips() {
        let resolved: InteractionResolved = roundtrip(&json!(
            {"request_id": "r_1", "status": "cancelled", "reason": "turn_ended"}));
        assert_eq!(resolved.reason, Some(ResolutionReason::TurnEnded));
        assert_eq!(resolved.status, InteractionStatus::Cancelled);
    }

    #[test]
    fn unit_proto_events_session_interaction_enums_are_open() {
        // `unknown` is a known value of these two, not the fallback.
        assert_eq!(EffectScope::from("unknown"), EffectScope::Unclassified);
        assert_eq!(PrefixRisk::from("unknown"), PrefixRisk::Unparsed);
        open_values::<OptionKind>(OptionKind::KNOWN);
        open_values::<EffectScope>(EffectScope::KNOWN);
        open_values::<PrefixRisk>(PrefixRisk::KNOWN);
        open_values::<FieldType>(FieldType::KNOWN);
        open_values::<InteractionStatus>(InteractionStatus::KNOWN);
        open_values::<ResolutionReason>(ResolutionReason::KNOWN);
    }
}
