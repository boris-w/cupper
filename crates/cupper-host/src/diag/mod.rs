//! Diagnostics (design 5.11): `host.log`, the adapters' stderr logs and the
//! redaction layer in its two tiers.

pub mod full;
pub mod log;
pub mod redact;
pub mod rotate;
pub mod stderr;
