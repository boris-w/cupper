//! Interaction requests (design 5.3.4, 5.3.6): answering one, and reading
//! one for the native confirmation.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use super::catalog::Catalog;
use crate::domain::ResolvedBy;
use crate::ids::{CommandId, Digest, RequestId, SessionId};
use crate::interaction::{
    InteractionOutcome, InteractionRequested, InteractionStatus, ResolutionReason,
};
use crate::open_enum;

method! {
    /// `interaction/respond`: first come, first served. The host checks the
    /// digest, `actionable_by`, and the option's `requires`; an option with
    /// `mode_after` also follows the tier rules (design 5.3.6, 5.8.2).
    request InteractionRespond = "interaction/respond" (InteractionRespondParams) -> InteractionRespondResult {
        scope: Approve, idempotency: Command, milestone: M1a,
        errors: [
            Conflict / AlreadyResolved, Conflict / DigestMismatch, Conflict / TooEarly,
            Expired, Forbidden,
        ],
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRespondParams {
    pub command_id: CommandId,
    pub session_id: SessionId,
    pub request_id: RequestId,
    /// The request's `digest`, echoed.
    pub request_digest: Digest,
    pub outcome: InteractionOutcome,
    /// Required from a `window` connection, ignored from other principals.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputEvidence>,
}

/// The user input behind a command, written by the desktop main process over
/// whatever the window sent (design 5.7.8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputEvidence {
    pub kind: InputKind,
    /// `kind: key` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<InputKey>,
    /// `key: digit` only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digit: Option<KeyDigit>,
}

open_enum! {
    pub enum InputKind {
        Click => "click",
        Key => "key",
        /// The user confirmed a native dialog.
        NativeConfirm => "native_confirm",
    }
}

open_enum! {
    pub enum InputKey {
        Enter => "enter",
        Space => "space",
        /// Only for `dismissed`, or an option whose effect is `reject` or `plan_revise`.
        Escape => "escape",
        CmdEnter => "cmd_enter",
        /// A digit key of the request panel (v1.4); see `digit`.
        Digit => "digit",
    }
}

/// A digit key, 1 to 9. For a `permission` or `plan_confirm` it must be the
/// number of the chosen option in `options`, from 1; the host checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct KeyDigit(u8);

/// A digit outside 1 to 9.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("digit {0} is not 1 to 9")]
pub struct DigitOutOfRange(pub u8);

impl TryFrom<u8> for KeyDigit {
    type Error = DigitOutOfRange;

    fn try_from(digit: u8) -> Result<Self, DigitOutOfRange> {
        match digit {
            1..=9 => Ok(Self(digit)),
            _ => Err(DigitOutOfRange(digit)),
        }
    }
}

impl From<KeyDigit> for u8 {
    fn from(digit: KeyDigit) -> u8 {
        digit.0
    }
}

impl JsonSchema for KeyDigit {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> Cow<'static, str> {
        "KeyDigit".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "integer", "minimum": 1, "maximum": 9 })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionRespondResult {
    pub request_id: RequestId,
}

method! {
    /// `interaction/get`: a request and its state, for the native confirmation.
    request InteractionGet = "interaction/get" (InteractionGetParams) -> InteractionGetResult {
        scope: Read, idempotency: Natural, milestone: M1a,
        errors: [NotFound],
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionGetParams {
    pub session_id: SessionId,
    pub request_id: RequestId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InteractionGetResult {
    /// As `interaction.requested` carried it.
    pub requested: InteractionRequested,
    pub status: InteractionStatus,
    /// Once no longer `pending`, as `interaction.resolved` carried them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<InteractionOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<ResolvedBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<ResolutionReason>,
}

pub fn register(c: &mut Catalog, g: &mut SchemaGenerator) {
    c.request::<InteractionRespond>(g);
    c.request::<InteractionGet>(g);
}

#[cfg(test)]
pub(super) mod tests {
    use serde_json::json;

    use super::*;
    use crate::methods::testkit::{digest, request};
    use crate::schema::generator;
    use crate::testutil::{open_values, roundtrip};

    /// Typical values of every method in this group (design 5.3.4, 5.3.6).
    pub fn samples() -> Vec<&'static str> {
        let respond = |outcome, input| {
            let mut params = json!({"command_id": "c-1", "session_id": "s_1", "request_id": "r_1",
                "request_digest": digest('f'), "outcome": outcome});
            if input != json!(null) {
                params["input"] = input;
            }
            request::<InteractionRespond>(params, json!({"request_id": "r_1"}))
        };
        let option = |id: &str, kind: &str, scope: &str| {
            json!({"option_id": id, "name": id, "kind": kind,
                "effect": {"scope": scope, "persistent": false}, "requires": "approve"})
        };
        let requested = json!({"request_id": "r_1", "turn_id": "t_1", "item_id": "i_1",
            "created_at": 1700000000000_u64, "digest": digest('f'),
            "actionable_by": ["desktop_ui", "cli", "test_harness"],
            "request": {"type": "permission", "title": "Run cargo test", "content_truncated": false,
                "tool": {"tool_kind": "execute", "command": {"type": "text", "text": "cargo test"},
                    "paths": []},
                "options": [option("allow-once", "allow_once", "once"),
                    option("reject", "reject_once", "reject")]}});
        let get = json!({"session_id": "s_1", "request_id": "r_1"});
        vec![
            respond(
                json!({"type": "selected", "option_id": "allow-once"}),
                json!({"kind": "key", "key": "digit", "digit": 1}),
            ),
            respond(
                json!({"type": "answer", "values": {"q1": "yes"}}),
                json!({"kind": "click"}),
            ),
            respond(
                json!({"type": "dismissed"}),
                json!({"kind": "key", "key": "escape"}),
            ),
            respond(
                json!({"type": "selected", "option_id": "reject"}),
                json!(null),
            ),
            request::<InteractionGet>(
                get.clone(),
                json!({"requested": requested, "status": "pending"}),
            ),
            request::<InteractionGet>(
                get,
                json!({"requested": requested, "status": "resolved",
                    "outcome": {"type": "selected", "option_id": "allow-once"},
                    "resolved_by": {"principal_id": "pr_cli", "client_kind": "cli",
                        "client_name": "cupper-cli"}}),
            ),
        ]
    }

    #[test]
    fn unit_proto_methods_core_session_input_digit() {
        // The request panel's digit keys (v1.4) are 1 to 9; nothing else parses.
        let input: InputEvidence = roundtrip(&json!({"kind": "key", "key": "digit", "digit": 9}));
        assert_eq!(input.key, Some(InputKey::Digit));
        assert_eq!(input.digit.map(u8::from), Some(9));
        for bad in [0, 10, 255] {
            assert_eq!(KeyDigit::try_from(bad), Err(DigitOutOfRange(bad)));
            let raw = json!({"kind": "key", "key": "digit", "digit": bad});
            let err = serde_json::from_value::<InputEvidence>(raw).unwrap_err();
            assert!(
                err.to_string()
                    .contains(&format!("digit {bad} is not 1 to 9")),
                "{err}"
            );
        }

        let mut g = generator();
        g.subschema_for::<InputEvidence>();
        let defs = g.take_definitions(true);
        let digit = &defs["InputEvidence"]["properties"]["digit"];
        assert_eq!(
            (&digit["minimum"], &digit["maximum"]),
            (&json!(1), &json!(9))
        );
        open_values::<InputKind>(InputKind::KNOWN);
        open_values::<InputKey>(InputKey::KNOWN);
    }
}
