//! The cupper session host (design 5.2). Modules wire themselves; this file only
//! declares them (plan 1.5).

pub mod clock;
pub mod diag;
pub mod driver;
pub mod durations;
pub mod elevation;
pub mod exit;
pub mod failpoint;
pub mod gateway;
pub mod git;
pub mod index;
pub mod ledger;
pub mod manifest;
pub mod orchestrator;
pub mod policy;
pub mod pure;
pub mod quiescence;
pub mod requester;
pub mod runtime;
pub mod startup;
pub mod store;
pub mod supervisor;
pub mod trust;
