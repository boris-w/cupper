//! `dev_key` authentication on the desktop's side (design 5.8.1): the
//! one-time key the desktop starts the host with, the startup payload that
//! carries it, and both proofs. The key never leaves the extension:
//! JavaScript only holds a handle to it.

use cupper_protocol::proof::{self, Bytes32, Channel, Direction};
use napi_derive::napi;
use serde_json::{Map, Value};

/// 32 random bytes; no `Debug`, so that it is never printed.
#[derive(Clone)]
pub struct DevKey(Bytes32);

/// `initialize`'s `auth` in a `dev-auth` build.
#[napi(object, object_from_js = false)]
#[derive(Debug)]
pub struct DevKeyAuth {
    /// Always `dev_key`.
    #[napi(js_name = "type")]
    pub kind: String,
    #[napi(js_name = "c_nonce")]
    pub c_nonce: String,
    pub proof: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KeyError {
    #[error("no random bytes: {0}")]
    Random(String),
    #[error("the startup payload sets `dev_key` itself")]
    PayloadHasKey,
    #[error("the nonce of `host/hello` is not 64 hex digits")]
    BadNonce,
}

/// 32 bytes from the system's random source.
pub fn random() -> Result<Bytes32, KeyError> {
    let mut bytes = [0; 32];
    getrandom::fill(&mut bytes).map_err(|err| KeyError::Random(err.to_string()))?;
    Ok(bytes)
}

impl DevKey {
    pub fn generate() -> Result<Self, KeyError> {
        random().map(Self)
    }

    /// The startup payload: `extra` (in e2e builds `harness_key` and
    /// `test_config`) with `dev_key`, as one line of JSON ending in a newline.
    pub fn payload(&self, mut extra: Map<String, Value>) -> Result<Vec<u8>, KeyError> {
        if extra.contains_key("dev_key") {
            return Err(KeyError::PayloadHasKey);
        }
        extra.insert("dev_key".into(), proof::to_hex(&self.0).into());
        let mut line = Value::Object(extra).to_string().into_bytes();
        line.push(b'\n');
        Ok(line)
    }

    /// The proof that process `pid` holds the key, bound to `h_nonce` from
    /// `host/hello` and to `c_nonce`; and the channel that `host_proof` is
    /// checked against.
    pub fn auth(
        &self,
        h_nonce: &str,
        c_nonce: Bytes32,
        pid: i32,
    ) -> Result<(DevKeyAuth, Channel), KeyError> {
        let h_nonce = proof::parse_hex32(h_nonce).ok_or(KeyError::BadNonce)?;
        let channel = Channel { h_nonce, c_nonce };
        let auth = DevKeyAuth {
            kind: "dev_key".into(),
            c_nonce: proof::to_hex(&c_nonce),
            proof: channel.prove(&self.0, Direction::ClientToHost, pid),
        };
        Ok((auth, channel))
    }

    /// Whether `host_proof` proves the key for `channel`, from the host whose
    /// pinned token names `pid` (T021: not `LOCAL_PEERPID`).
    pub fn holds(&self, channel: &Channel, pid: i32, host_proof: Option<&str>) -> bool {
        host_proof.is_some_and(|host_proof| {
            let checked = channel.check(&self.0, Direction::HostToClient, pid, host_proof);
            checked.is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_native_proof_binding_known_answer() {
        // The known answers of UNIT-proof-binding (`cupper_protocol::proof`),
        // computed independently, through this extension's path: the nonce
        // as `host/hello` sends it, the pid as the token gives it.
        let key = DevKey(std::array::from_fn(|i| i as u8));
        let pid = 0x0102_0304;
        let (auth, channel) = key.auth(&"aa".repeat(32), [0xbb; 32], pid).unwrap();
        assert_eq!(
            (auth.kind.as_str(), auth.c_nonce),
            ("dev_key", "bb".repeat(32))
        );
        assert_eq!(
            auth.proof,
            "6851285dda55129ca1b078ea1150d3772c4a6442fb90ab04a0f788b47c0553be"
        );
        let host_proof = "70708b5e0e8be2cbf88f3a3f1d2c0b68b2723898d29037b2734907ac5478a6af";
        assert!(key.holds(&channel, pid, Some(host_proof)));
        assert!(key.holds(&channel, pid, Some(&host_proof.to_uppercase())));

        // The host's proof from another pid, of the client's direction, of
        // another key, or none at all does not hold.
        let other = DevKey([7; 32]);
        assert!(!key.holds(&channel, pid + 1, Some(host_proof)));
        assert!(!key.holds(&channel, pid, Some(&auth.proof)));
        assert!(!other.holds(&channel, pid, Some(host_proof)));
        assert!(!key.holds(&channel, pid, None));
        assert!(!key.holds(&channel, pid, Some("70708b5e")));
        let bad = key.auth("aa", [0xbb; 32], pid).unwrap_err();
        assert_eq!(bad, KeyError::BadNonce);
    }

    #[test]
    fn unit_native_payload_is_one_line_with_the_key() {
        let key = DevKey::generate().unwrap();
        assert_ne!(key.0, DevKey::generate().unwrap().0);
        let mut extra = Map::new();
        extra.insert("harness_key".into(), "cd".repeat(32).into());
        let line = key.payload(extra.clone()).unwrap();
        let (json, end) = line.split_at(line.len() - 1);
        assert_eq!(end, b"\n");
        assert!(!json.contains(&b'\n'));
        let mut expected = extra.clone();
        expected.insert("dev_key".into(), proof::to_hex(&key.0).into());
        let parsed: Map<String, Value> = serde_json::from_slice(json).unwrap();
        assert_eq!(parsed, expected);

        extra.insert("dev_key".into(), "00".repeat(32).into());
        assert_eq!(key.payload(extra), Err(KeyError::PayloadHasKey));
    }
}
