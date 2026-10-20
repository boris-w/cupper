//! `cupper-fake-agent`: a scripted ACP agent for the host's integration and
//! end-to-end tests (design 5.9.2). It is a test tool and is never shipped in
//! the app (5.10.1).
//!
//! - [`config`]: the start configuration (persona, login state, faults,
//!   process segment), from the command line;
//! - [`persona`]: what Claude's and Codex's adapters look like on the wire;
//! - [`agent`]: the SDK's agent role, sessions and the global invariant;
//! - [`script`]: the prompt text is the script;
//! - [`replay`]: or a recording is played instead.
//!
//! Everything goes through the SDK except the lines of the `raw` command
//! and of a replay, which reach stdout verbatim.

pub mod agent;
pub mod config;
pub mod persona;
pub mod replay;
pub mod script;
