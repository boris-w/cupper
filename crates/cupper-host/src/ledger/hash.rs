//! The parameter hash (design 5.4.2, `commands.params_hash`).

use cupper_protocol::canonical::{self, CanonicalError};
use cupper_protocol::ids::Digest;
use serde_json::{Map, Value};

/// Top-level parameters a repeat may change: the command's own ID, the M3
/// attestation, and the main process's input evidence, which differs from one
/// key press to the next.
const UNHASHED: [&str; 3] = ["command_id", "attestation", "input"];

/// `d_` + the sha-256 of the RFC 8785 form of `params` without its
/// top-level `command_id`, `attestation` and `input`, so neither those nor key
/// order nor whitespace counts.
pub fn params_hash(params: &Value) -> Result<Digest, CanonicalError> {
    let Value::Object(members) = params else {
        return canonical::digest(params);
    };
    let hashed: Map<String, Value> = members
        .iter()
        .filter(|(name, _)| !UNHASHED.contains(&name.as_str()))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    canonical::digest(&hashed)
}
