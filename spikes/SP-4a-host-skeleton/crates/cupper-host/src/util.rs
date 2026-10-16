//! Ids, time and canonical hashing.

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Prefixed UUIDv7 id, e.g. `s_0190...`.
pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", uuid::Uuid::now_v7().simple())
}

/// UTC milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// JSON with object keys sorted recursively, so the hash does not depend on
/// the key order a client happened to use.
fn canonical(v: &Value, out: &mut String) {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort();
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("string serializes"));
                out.push(':');
                canonical(&m[k], out);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                canonical(x, out);
            }
            out.push(']');
        }
        other => out.push_str(&other.to_string()),
    }
}

/// Hex sha-256 of the canonical JSON form.
pub fn canonical_hash(v: &Value) -> String {
    let mut s = String::new();
    canonical(v, &mut s);
    hex::encode(Sha256::digest(s.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_ignores_key_order() {
        let a: Value = serde_json::from_str(r#"{"b":1,"a":{"y":[1,{"q":2,"p":3}],"x":null}}"#).unwrap();
        let b: Value = serde_json::from_str(r#"{"a":{"x":null,"y":[1,{"p":3,"q":2}]},"b":1}"#).unwrap();
        assert_eq!(canonical_hash(&a), canonical_hash(&b));
    }
}
