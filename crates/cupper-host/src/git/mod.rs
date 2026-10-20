//! Git (design 5.2.10, ADR-10): every call through the git command line, in
//! three tiers by where it runs. Here so far: the repo tier's rules and the
//! fingerprint of a repository's executable configuration.

pub mod rules;
pub mod stamp;
