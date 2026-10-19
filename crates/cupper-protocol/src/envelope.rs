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
use crate::open_enum;

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

/// Longest [`Fallback::summary`], in characters.
pub const MAX_FALLBACK_SUMMARY_CHARS: usize = 120;

/// What a reader shows in place of an event type it does not know (design 5.3.11).
///
/// Only a client older than the event type reads it, and such a client has no
/// translation for the semantic codes the new type brings, so a code would not
/// help. The client draws its own localized placeholder of shape `kind`;
/// `summary` appears only in a tooltip or in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Fallback {
    pub kind: FallbackKind,
    /// English for developers, at most [`MAX_FALLBACK_SUMMARY_CHARS`]
    /// characters, never agent-provided text or paths.
    pub summary: String,
}

impl Fallback {
    pub fn new(kind: FallbackKind, summary: &str) -> Self {
        debug_assert!(
            summary.chars().count() <= MAX_FALLBACK_SUMMARY_CHARS,
            "fallback summary too long: {summary}"
        );
        Self {
            kind,
            summary: summary.to_owned(),
        }
    }
}

open_enum! {
    /// The placeholder shape a client already knows how to draw.
    pub enum FallbackKind {
        /// A row in the message stream.
        Item => "item",
        /// Nothing: the event only changes state the client cannot show.
        Hidden => "hidden",
    }
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
            "fallback":{"kind":"item","summary":"New thing"}}"#;
        let e: Event<Body> = serde_json::from_str(raw).unwrap();
        assert!(matches!(&e.body, Body::Unknown(u) if u.kind == "future.thing"));
        assert_eq!(
            e.fallback,
            Some(Fallback::new(FallbackKind::Item, "New thing"))
        );
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

    #[test]
    fn unit_proto_events_index_fallback_kind_is_open() {
        let raw = serde_json::json!({"kind": "card", "summary": "Shows a card"});
        let f: Fallback = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(f.kind, FallbackKind::Unknown("card".into()));
        assert_eq!(serde_json::to_value(&f).unwrap(), raw);
        assert_eq!(FallbackKind::KNOWN, ["item", "hidden"]);
    }

    #[test]
    #[should_panic(expected = "fallback summary too long")]
    fn unit_proto_events_index_fallback_summary_is_short() {
        Fallback::new(
            FallbackKind::Item,
            &"x".repeat(MAX_FALLBACK_SUMMARY_CHARS + 1),
        );
    }
}
