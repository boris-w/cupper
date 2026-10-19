//! Helpers for the method samples of each group.

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

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
