//! Event envelope (design 5.3.5, ADR-06): `{seq, from?, type, v, ts, cause?, fallback?, data}`.
//!
//! The body `B` supplies `type` and `data`: an open union declared with
//! [`open_union!`](crate::open_union!) in its `tag "type", content "data"`
//! form, whose unknown member is an [`UnknownEvent`]. A reader shows
//! `fallback` for a type it does not know.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::annotations::TimestampMs;
use crate::ids::{CommandId, PrincipalId};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Event<B> {
    /// Position in its stream; contiguous in the host log.
    pub seq: u64,
    /// Set on a range event made by send-time coalescing: it covers `from..=seq`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<u64>,
    /// Schema version of this event type.
    pub v: u32,
    pub ts: TimestampMs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<Cause>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Fallback>,
    #[serde(flatten)]
    pub body: B,
}

impl<B> Event<B> {
    /// The first seq this event covers; readers treat a missing `from` as `seq`.
    pub fn first_seq(&self) -> u64 {
        self.from.unwrap_or(self.seq)
    }
}

/// The command behind an event, so a client recognizes the outcome of its own commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cause {
    pub command_id: CommandId,
    pub principal: PrincipalId,
}

/// What a reader shows in place of an event type it does not know.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Fallback {
    pub kind: String,
    pub summary: String,
}

/// Body of an event whose `type` this version does not know.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnknownEvent {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::TurnId;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type", content = "data")]
    enum Body {
        #[serde(rename = "turn.started")]
        TurnStarted { turn_id: TurnId },
        #[serde(untagged)]
        Unknown(UnknownEvent),
    }

    #[test]
    fn unit_proto_base_envelope_keeps_unknown_types() {
        let raw = r#"{"seq":5,"v":3,"ts":1,"type":"future.thing","data":{"x":1},
            "fallback":{"kind":"card","summary":"New thing"}}"#;
        let e: Event<Body> = serde_json::from_str(raw).unwrap();
        assert!(matches!(&e.body, Body::Unknown(u) if u.kind == "future.thing"));
        assert_eq!(e.first_seq(), 5);
        let back = serde_json::to_value(&e).unwrap();
        assert_eq!(back, serde_json::from_str::<Value>(raw).unwrap());

        let raw = r#"{"seq":7,"from":6,"v":1,"ts":2,"type":"turn.started","data":{"turn_id":"t_1"},
            "cause":{"command_id":"c-1","principal":"pr_cli"}}"#;
        let e: Event<Body> = serde_json::from_str(raw).unwrap();
        assert_eq!(
            e.body,
            Body::TurnStarted {
                turn_id: "t_1".parse().unwrap()
            }
        );
        assert_eq!(
            (e.first_seq(), e.cause.unwrap().principal.as_str()),
            (6, "pr_cli")
        );
    }
}
