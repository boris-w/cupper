//! `data_epoch` (design 5.2.8, 5.9.8).
//!
//! `synchronous=NORMAL` survives a process crash but not a power loss, which
//! may drop the latest transactions. The host then hands out those seqs again,
//! and a client's cursor would point at events that no longer exist. So at
//! startup the host bumps `meta.data_epoch` when the previous run may have
//! lost data that way, and clients whose cursor carries an older epoch take a
//! snapshot (design 5.3.3).
//!
//! A power loss shows as both an unclean exit and a new boot. `kill -9` is an
//! unclean exit on the same boot: the WAL keeps every committed transaction,
//! so the epoch stays. `kill -9` followed by an ordinary reboot looks like a
//! power loss and costs the clients one full snapshot.
//!
//! [`decide`] is the pure decision. Its inputs come from `meta` (written by
//! the startup sequence) and from [`current_boot_session`], which a
//! `test-harness` build lets `test_config.boot_session_uuid` override.

use std::fmt;

use uuid::Uuid;

use crate::startup::payload::Payload;

/// How the host's previous run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastExit {
    /// There was no previous run: the database is new.
    FirstStart,
    /// The orderly exit, which writes `meta.last_clean_exit_at` last.
    Clean,
    /// Anything else: a crash, `kill -9`, a power loss.
    Unclean,
}

impl LastExit {
    /// From the previous run's `meta.last_start_at` and
    /// `meta.last_clean_exit_at` (UTC ms). A run that did not exit cleanly left
    /// the clean exit of an earlier run, older than its own start, or none.
    pub fn from_meta(last_start_at: Option<u64>, last_clean_exit_at: Option<u64>) -> Self {
        match (last_start_at, last_clean_exit_at) {
            (None, _) => Self::FirstStart,
            (Some(start), Some(exit)) if exit >= start => Self::Clean,
            (Some(_), _) => Self::Unclean,
        }
    }
}

/// Whether the machine has booted since the previous run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boot {
    Same,
    Changed,
}

impl Boot {
    /// Compares the boot the previous run recorded (`meta.boot_session_uuid`)
    /// with the current one. A missing or unreadable record counts as a
    /// reboot: at worst, the clients take one extra snapshot.
    pub fn since(previous: Option<&str>, current: BootSession) -> Self {
        match previous.map(BootSession::parse) {
            Some(Ok(previous)) if previous == current => Self::Same,
            _ => Self::Changed,
        }
    }
}

/// What startup does with `data_epoch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EpochStep {
    Keep,
    /// Add 1.
    Bump,
}

/// Bumps the epoch only after an unclean exit across a reboot: the only case
/// in which committed transactions may be gone.
pub fn decide(last_exit: LastExit, boot: Boot) -> EpochStep {
    match (last_exit, boot) {
        (LastExit::Unclean, Boot::Changed) => EpochStep::Bump,
        _ => EpochStep::Keep,
    }
}

/// The UUID macOS gives the current boot (`kern.bootsessionuuid`). It
/// displays in the kernel's upper-case form, which is how `meta` records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BootSession(Uuid);

impl BootSession {
    pub fn parse(text: &str) -> Result<Self, BootSessionError> {
        Uuid::try_parse(text)
            .map(Self)
            .map_err(|_| BootSessionError::NotUuid(text.to_owned()))
    }
}

impl fmt::Display for BootSession {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:X}", self.0)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BootSessionError {
    #[error("cannot read kern.bootsessionuuid")]
    System(#[from] cupper_sys::Error),
    #[error("`{0}` is not a boot session UUID")]
    NotUuid(String),
}

/// The current boot: `test_config.boot_session_uuid` when a `test-harness`
/// build was given one, else `kern.bootsessionuuid`.
pub fn current_boot_session(payload: &Payload) -> Result<BootSession, BootSessionError> {
    #[cfg(feature = "test-harness")]
    if let Some(injected) = &payload.test_config.boot_session_uuid {
        return BootSession::parse(injected);
    }
    #[cfg(not(feature = "test-harness"))]
    let _ = payload;
    BootSession::parse(&cupper_sys::boot_session_uuid()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOOT_A: &str = "8D4E1A0C-0000-4000-8000-00000000000A";
    const BOOT_B: &str = "8D4E1A0C-0000-4000-8000-00000000000B";

    fn boot(text: &str) -> BootSession {
        BootSession::parse(text).unwrap()
    }

    #[test]
    fn unit_epoch_decision_bumps_only_after_an_unclean_exit_across_a_reboot() {
        use {Boot::*, EpochStep::*, LastExit::*};
        let table = [
            (FirstStart, Same, Keep),
            (FirstStart, Changed, Keep),
            (Clean, Same, Keep),
            (Clean, Changed, Keep),
            // `kill -9` or a crash: the WAL lost nothing.
            (Unclean, Same, Keep),
            // A power loss.
            (Unclean, Changed, Bump),
        ];
        for (last_exit, boot, step) in table {
            assert_eq!(decide(last_exit, boot), step, "{last_exit:?} {boot:?}");
        }
    }

    #[test]
    fn unit_epoch_decision_reads_its_inputs_from_meta() {
        assert_eq!(LastExit::from_meta(None, None), LastExit::FirstStart);
        assert_eq!(LastExit::from_meta(Some(1000), None), LastExit::Unclean);
        // The previous run's start is newer than the last clean exit.
        assert_eq!(
            LastExit::from_meta(Some(1000), Some(999)),
            LastExit::Unclean
        );
        // A virtual clock may not move between start and exit.
        assert_eq!(LastExit::from_meta(Some(1000), Some(1000)), LastExit::Clean);
        assert_eq!(LastExit::from_meta(Some(1000), Some(5000)), LastExit::Clean);

        let current = boot(BOOT_A);
        assert_eq!(Boot::since(Some(BOOT_A), current), Boot::Same);
        assert_eq!(
            Boot::since(Some(&BOOT_A.to_lowercase()), current),
            Boot::Same
        );
        assert_eq!(Boot::since(Some(BOOT_B), current), Boot::Changed);
        assert_eq!(Boot::since(None, current), Boot::Changed);
        assert_eq!(Boot::since(Some("not a uuid"), current), Boot::Changed);
        assert_eq!(current.to_string(), BOOT_A);
    }

    /// Case ④ of design 5.9.8: `kill -9`, then the user restarts the
    /// computer before the host runs again.
    #[test]
    fn unit_epoch_decision_kill_9_then_reboot_counts_as_power_loss() {
        // The killed run started at 2000; the clean exit before it was at 1500.
        let last_exit = LastExit::from_meta(Some(2000), Some(1500));
        let current = boot(BOOT_A);
        assert_eq!(
            decide(last_exit, Boot::since(Some(BOOT_A), current)),
            EpochStep::Keep
        );
        assert_eq!(
            decide(last_exit, Boot::since(Some(BOOT_B), current)),
            EpochStep::Bump
        );
    }

    #[test]
    fn unit_epoch_decision_boot_session_comes_from_the_kernel() {
        let kernel = cupper_sys::boot_session_uuid().unwrap();
        let current = current_boot_session(&Payload::default()).unwrap();
        assert_eq!(current.to_string(), kernel);
        assert!(matches!(
            BootSession::parse("8D4E1A0C"),
            Err(BootSessionError::NotUuid(text)) if text == "8D4E1A0C"
        ));
    }

    #[cfg(feature = "test-harness")]
    #[test]
    fn unit_epoch_decision_boot_session_injected_by_test_config() {
        let mut payload = Payload::default();
        payload.test_config.boot_session_uuid = Some(BOOT_B.to_lowercase());
        assert_eq!(current_boot_session(&payload).unwrap(), boot(BOOT_B));
        payload.test_config.boot_session_uuid = Some("next boot".to_owned());
        assert!(matches!(
            current_boot_session(&payload),
            Err(BootSessionError::NotUuid(_))
        ));
    }
}
