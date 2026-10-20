//! macOS system interfaces for cupper (ADR-02, design 5.1.3).
//!
//! Every `unsafe` block of the workspace lives in this crate, so the rest of the
//! code base stays `unsafe`-free and the safety arguments can be reviewed in one
//! place. Each block states its preconditions in a `SAFETY:` comment.

#![allow(
    unsafe_code,
    reason = "cupper-sys is the one crate that calls macOS system interfaces; the workspace denies `unsafe` everywhere else (design 5.1.1)"
)]
#![deny(clippy::undocumented_unsafe_blocks)]

mod codesign;
mod env;
mod exit;
mod group;
mod peer;
mod proc;
mod runloop;
mod spawn;
mod system;
#[cfg(test)]
mod testutil;

pub use codesign::{
    Cdhash, CodeStatus, Requirement, SigningIdentity, file_cdhash, own_cdhash, pid_cdhash,
};
pub use env::Env;
pub use exit::ExitWatch;
pub use group::{ProcessGroup, Signal};
#[cfg(any(test, feature = "test-util"))]
pub use peer::send_with_fds;
pub use peer::{
    AuditToken, Received, Refusal, RefusalKind, peer_pid, peer_token, peer_uid, recv_closing_fds,
};
pub use proc::{
    ProcessInfo, StartTime, UniqueIds, descendants, has_tty, parent_chain, pid_path, process_info,
    resident_size, unique_ids,
};
pub use runloop::{RunLoop, RunLoopStop};
pub use spawn::{Child, Command, Stdio, spawn};
pub use system::{
    boot_session_uuid, current_uid, host_name, redirect_stdio_to_null, setsid, user_temp_dir,
};

/// Errors from system interfaces.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The caller passed a value the system interface cannot take.
    #[error("invalid argument: {0}")]
    InvalidArgument(&'static str),
    /// A system interface returned data in a form it never should.
    #[error("unexpected result from {0}")]
    Unexpected(&'static str),
    /// A Security framework call returned this `OSStatus`.
    #[error("{call} returned OSStatus {status}")]
    Security { call: &'static str, status: i32 },
    /// A system call failed.
    #[error("{call} failed")]
    Os {
        call: &'static str,
        #[source]
        source: std::io::Error,
    },
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// `call` failed with the error code in `errno`.
    pub(crate) fn last_os(call: &'static str) -> Self {
        Self::Os {
            call,
            source: std::io::Error::last_os_error(),
        }
    }
}
