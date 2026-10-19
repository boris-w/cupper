//! The host's exit codes, part of its startup contract (design 5.2.13, frozen
//! list 5.12.5 #9). The desktop launcher acts on them (design 5.7.1,
//! `hostLauncher`): 3 connects to the running host instead, 4, 5 and 6 explain
//! the problem to the user.

use std::process::ExitCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostExit {
    /// 0: a normal exit.
    Ok,
    /// 1: any other error, including a startup payload that timed out or was
    /// malformed (design 5.8.1) and a command line the host does not accept.
    Error,
    /// 3: another host holds `host.lock`.
    AlreadyRunning,
    /// 4: the database comes from a newer host; there are no downgrades (NFR-10).
    DataTooNew,
    /// 5: the data directory cannot be used.
    DataDirUnusable,
    /// 6: a migration failed; the UI points at the backup.
    MigrationFailed,
    /// 99: a failpoint fired (design 5.9.8). Only `test-harness` builds have
    /// failpoints; the code stays clear of the ones above and of 101, Rust's
    /// panic exit.
    #[cfg(feature = "test-harness")]
    Failpoint,
}

impl HostExit {
    pub const fn code(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Error => 1,
            Self::AlreadyRunning => 3,
            Self::DataTooNew => 4,
            Self::DataDirUnusable => 5,
            Self::MigrationFailed => 6,
            #[cfg(feature = "test-harness")]
            Self::Failpoint => 99,
        }
    }
}

impl From<HostExit> for ExitCode {
    fn from(exit: HostExit) -> Self {
        Self::from(exit.code())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_codes_match_the_startup_contract() {
        let contract = [
            (HostExit::Ok, 0),
            (HostExit::Error, 1),
            (HostExit::AlreadyRunning, 3),
            (HostExit::DataTooNew, 4),
            (HostExit::DataDirUnusable, 5),
            (HostExit::MigrationFailed, 6),
        ];
        for (exit, code) in contract {
            assert_eq!(exit.code(), code, "{exit:?}");
        }
    }
}
