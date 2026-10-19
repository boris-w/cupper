//! Helpers for the method samples of each group.

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use crate::canonical::to_canonical_string;
use crate::rpc::{Method, Notification};
use crate::testutil::roundtrip;

/// `raw` survives JSON → Rust → JSON and Rust → JSON → Rust, and has only
/// integers within ±(2^53 − 1), which canonical JSON checks.
fn both_ways<T: Serialize + DeserializeOwned + PartialEq + Debug>(raw: Value) {
    let parsed: T = roundtrip(&raw);
    let again: T = serde_json::from_value(serde_json::to_value(&parsed).unwrap()).unwrap();
    assert_eq!(again, parsed);
    to_canonical_string(&raw).unwrap_or_else(|e| panic!("{raw}: {e}"));
}

/// Checks typical params and result of `M`; returns its name.
pub fn request<M: Method>(params: Value, result: Value) -> &'static str
where
    M::Params: PartialEq + Debug,
    M::Result: PartialEq + Debug,
{
    both_ways::<M::Params>(params);
    both_ways::<M::Result>(result);
    M::NAME
}

/// Checks typical params of `N`; returns its name.
pub fn notification<N: Notification>(params: Value) -> &'static str
where
    N::Params: PartialEq + Debug,
{
    both_ways::<N::Params>(params);
    N::NAME
}

/// A digest whose 64 hex digits are all `c`.
pub fn digest(c: char) -> String {
    format!("d_{}", c.to_string().repeat(64))
}

/// Typical capabilities of a Claude session (design 5.3.7).
pub fn capabilities() -> Value {
    json!({"agent": "claude",
        "versions": {"adapter": "0.91.0", "cli": "2.1.296", "cli_compat": "verified"},
        "prompt": {"image": true, "file_ref": true, "max_image_bytes": 10485760},
        "turn": {"queue": true, "steer": {"supported": true, "side_effects": true},
            "interrupt": true, "interrupt_stops_commands": true},
        "session": {"resume": true, "load_history": false, "fork": false},
        "config": [{"id": "model", "agent_option_id": "model", "current": "opus",
            "values": [{"value": "opus", "name": "Opus", "risk": "normal"}]}],
        "commands": [{"name": "review", "description": "Review a PR"}],
        "interactions": {"reject_ends_turn": false, "question_forms": true, "plan_confirm": true},
        "extraction": {"exit_code": true, "output_streaming": false, "diff": "per_hunk",
            "thinking": true, "plan": "markdown_only", "run_state": "inferred"}})
}
