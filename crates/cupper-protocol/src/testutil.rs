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

/// Every integer schema in `v`, wherever it is nested.
pub fn integers<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => {
            let t = m.get("type");
            if t == Some(&json!("integer"))
                || t.and_then(Value::as_array)
                    .is_some_and(|a| a.contains(&json!("integer")))
            {
                out.push(v);
            }
            m.values().for_each(|c| integers(c, out));
        }
        Value::Array(a) => a.iter().for_each(|c| integers(c, out)),
        _ => {}
    }
}
