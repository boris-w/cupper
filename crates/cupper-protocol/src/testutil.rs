//! Helpers for the crate's unit tests.

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// Parses `raw` as `T` and checks that it serializes back to `raw`.
pub fn roundtrip<T: Serialize + DeserializeOwned>(raw: &Value) -> T {
    let parsed: T = serde_json::from_value(raw.clone()).unwrap_or_else(|e| panic!("{raw}: {e}"));
    assert_eq!(&serde_json::to_value(&parsed).unwrap(), raw);
    parsed
}

/// Every known value of an open enum, and one unknown value, survive a roundtrip.
pub fn open_values<T: Serialize + DeserializeOwned>(known: &[&str]) {
    let mut wire = known.to_vec();
    wire.push("zz_future");
    roundtrip::<Vec<T>>(&json!(wire));
}
