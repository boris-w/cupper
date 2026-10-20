//! Diagnostics (design 5.11): `host.log`, the adapters' stderr logs and the
//! redaction layer in its two tiers; in test and development builds, the
//! export of recordings (design 5.9.3).

pub mod full;
pub mod log;
#[cfg(any(feature = "test-harness", feature = "dev-auth"))]
pub mod recording_export;
pub mod redact;
pub mod rotate;
pub mod stderr;
