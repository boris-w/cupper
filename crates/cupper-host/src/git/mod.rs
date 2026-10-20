//! Git (design 5.2.10, ADR-10): every call through the git command line, in
//! three tiers by where it runs. Here so far: the repo tier's rules.

pub mod rules;
