//! `cupper-fake-agent`: a scripted ACP agent for the host's integration and
//! end-to-end tests (design 5.9.2). It is a test tool and is never shipped in
//! the app (5.10.1).
//!
//! - [`config`]: the start configuration (persona, login state, faults,
//!   process segment), from the command line.

pub mod config;
