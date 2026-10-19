//! Prefixed IDs (design 5.4.4), one type per kind so that IDs of different
//! kinds cannot be mixed up. Clients treat the text as opaque.
//!
//! Parsing checks the prefix and a short, URL-safe body, not the exact layout
//! the host generates today: the host can reject a malformed ID with a clear
//! error, and clients do not depend on the layout.

use std::borrow::Cow;
use std::fmt;
use std::str::FromStr;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::annotations::X_ID;

/// Longest ID body after the prefix (design 5.4.4: command IDs up to 64 characters).
pub const MAX_ID_BODY: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("malformed {kind}")]
pub struct IdError {
    pub kind: &'static str,
}

fn check(kind: &'static str, prefix: &str, text: &str) -> Result<(), IdError> {
    let body_ok = |body: &str| {
        (1..=MAX_ID_BODY).contains(&body.len())
            && body
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    };
    match text.strip_prefix(prefix) {
        Some(body) if body_ok(body) => Ok(()),
        _ => Err(IdError { kind }),
    }
}

fn id_schema(description: &str, varies: bool) -> Schema {
    let mut schema = json_schema!({ "type": "string", "description": description.trim() });
    if varies {
        schema.insert(X_ID.into(), true.into());
    }
    schema
}

macro_rules! ids {
    ($( #[doc = $doc:literal] $name:ident = $prefix:literal, varies: $varies:literal; )+) => {$(
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(String);

        impl $name {
            pub const PREFIX: &'static str = $prefix;

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(text: &str) -> Result<Self, IdError> {
                check(stringify!($name), $prefix, text)?;
                Ok(Self(text.to_owned()))
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let text = String::deserialize(d)?;
                check(stringify!($name), $prefix, &text).map_err(serde::de::Error::custom)?;
                Ok(Self(text))
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                id_schema($doc, $varies)
            }
        }
    )+};
}

ids! {
    /// Host: `h_` + UUIDv4, chosen on first run.
    HostId = "h_", varies: true;
    /// Project: `p_` + UUIDv7.
    ProjectId = "p_", varies: true;
    /// Session: `s_` + UUIDv7.
    SessionId = "s_", varies: true;
    /// Turn: `t_` + UUIDv7.
    TurnId = "t_", varies: true;
    /// Item: `i_` + UUIDv7.
    ItemId = "i_", varies: true;
    /// Interaction request: `r_` + UUIDv7.
    RequestId = "r_", varies: true;
    /// Queued message: `q_` + UUIDv7.
    QueueId = "q_", varies: true;
    /// Adapter instance: `a_` + UUIDv7.
    AdapterInstanceId = "a_", varies: true;
    /// Elevation request: `e_` + UUIDv7.
    ElevationId = "e_", varies: true;
    /// Principal: stable across connections (`pr_desktop`, `pr_cli`, `pr_test`, `pr_dev_<device>`).
    PrincipalId = "pr_", varies: false;
    /// Connection: `c_` + a counter within one host run.
    ConnId = "c_", varies: true;
    /// Client-generated UUIDv7 that makes a command idempotent; no prefix.
    CommandId = "", varies: true;
    /// Digest: `d_` + the full sha-256 of the RFC 8785 form, in hex.
    Digest = "d_", varies: true;
    /// Hunk: `hk_` + 24 hex digits of a content hash.
    HunkId = "hk_", varies: true;
    /// Diff token: `dt_` + random; names one diff in the host's cache.
    DiffToken = "dt_", varies: true;
    /// Notification intent: `n_` + UUIDv7.
    IntentId = "n_", varies: true;
    /// Activity group of consecutive tool calls: `g_` + UUIDv7.
    GroupId = "g_", varies: true;
    /// Blob: the sha-256 of its bytes in hex; no prefix. Varies when the content holds per-run values.
    BlobHash = "", varies: true;
}

macro_rules! from_uuid {
    ($($name:ident),+) => {$(
        impl $name {
            /// The ID whose body is `uuid` as 32 lowercase hex digits.
            pub fn from_uuid(uuid: u128) -> Self {
                Self(format!("{}{uuid:032x}", Self::PREFIX))
            }
        }
    )+};
}

from_uuid!(
    HostId,
    ProjectId,
    SessionId,
    TurnId,
    ItemId,
    RequestId,
    QueueId,
    AdapterInstanceId,
    ElevationId,
    IntentId,
    GroupId
);

impl ConnId {
    pub fn from_index(index: u64) -> Self {
        Self(format!("c_{index}"))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

impl Digest {
    pub fn from_sha256(hash: &[u8; 32]) -> Self {
        Self(format!("d_{}", hex(hash)))
    }
}

impl BlobHash {
    pub fn from_sha256(hash: &[u8; 32]) -> Self {
        Self(hex(hash))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_proto_base_ids_check_prefix_and_body() {
        let s = SessionId::from_uuid(42);
        assert_eq!(s.as_str(), format!("s_{}2a", "0".repeat(30)));
        assert_eq!(s.as_str().parse::<SessionId>().unwrap(), s);
        assert_eq!(ConnId::from_index(7).as_str(), "c_7");
        assert_eq!(
            Digest::from_sha256(&[0xab; 32]).as_str(),
            format!("d_{}", "ab".repeat(32))
        );
        assert!("pr_dev_mac-1".parse::<PrincipalId>().is_ok());
        assert!(
            "0190a3b4-0000-7000-8000-00000000002a"
                .parse::<CommandId>()
                .is_ok()
        );

        let long = format!("s_{}", "a".repeat(MAX_ID_BODY + 1));
        for bad in ["t_1", "s_", "s_a b", "s_é", "S_1", long.as_str()] {
            assert_eq!(
                bad.parse::<SessionId>(),
                Err(IdError { kind: "SessionId" }),
                "{bad}"
            );
        }
        // Neighbouring prefixes do not leak into each other.
        assert!("pr_cli".parse::<ProjectId>().is_err());
        assert!("dt_x".parse::<Digest>().is_err());
        assert!("hk_x".parse::<HostId>().is_err());
        assert!("".parse::<CommandId>().is_err());
    }

    #[test]
    fn unit_proto_base_ids_serde_roundtrip_and_reject() {
        let t: TurnId = serde_json::from_str("\"t_01\"").unwrap();
        assert_eq!(serde_json::to_string(&t).unwrap(), "\"t_01\"");
        let err = serde_json::from_str::<TurnId>("\"i_01\"").unwrap_err();
        assert_eq!(err.to_string(), "malformed TurnId");
    }

    #[test]
    fn unit_proto_events_index_new_ids() {
        assert_eq!(
            IntentId::from_uuid(1).as_str(),
            format!("n_{}1", "0".repeat(31))
        );
        assert!("g_01".parse::<GroupId>().is_ok());
        assert!("n_01".parse::<GroupId>().is_err());
        let hash = BlobHash::from_sha256(&[0x0f; 32]);
        assert_eq!(hash.as_str(), "0f".repeat(32));
        assert_eq!(hash.as_str().parse::<BlobHash>().unwrap(), hash);
    }
}
