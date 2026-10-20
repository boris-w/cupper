//! The durations behind the host's timers (design 5.2.16), in one registry.
//!
//! A timer reads its duration with [`Durations::get`] and a key registered
//! below, which carries the design value. In `test-harness` builds a test can
//! override any registered duration through `test_config.durations` (design
//! 5.9.9) and so run on short durations; release builds have no overrides. A
//! task that adds a timer registers its duration here in the same change
//! (plan 1.4).

#[cfg(feature = "test-harness")]
use std::collections::BTreeMap;
use std::time::Duration;

use cupper_protocol::limits::V1;

const DAY_SECS: u64 = 24 * 3600;

/// A registered duration: its name in `test_config.durations` and its value
/// in the design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DurationKey {
    name: &'static str,
    design_value: Duration,
}

impl DurationKey {
    pub const fn name(self) -> &'static str {
        self.name
    }

    pub const fn design_value(self) -> Duration {
        self.design_value
    }
}

macro_rules! durations {
    ($(
        $(#[doc = $doc:literal])*
        $(#[cfg($cfg:meta)])?
        $key:ident = $name:literal, $value:expr;
    )*) => {
        $(
            $(#[doc = $doc])*
            $(#[cfg($cfg)])?
            pub const $key: DurationKey = DurationKey { name: $name, design_value: $value };
        )*

        /// Every registered duration.
        pub const ALL: &[DurationKey] = &[$($(#[cfg($cfg)])? $key,)*];
    };
}

durations! {
    /// How long the store writer waits, once, for more operations after its
    /// queue runs empty (design 5.2.8). Only on `SystemClock`; it is not a
    /// `Clock` timer (design 5.2.16).
    STORE_BATCH_LINGER = "store_batch_linger", Duration::from_millis(2);
    /// How long a closing connection may take to send its last replies, such
    /// as the error that ends a handshake (design 5.3.11, rulings before T030).
    GATEWAY_CLOSE_GRACE = "gateway_close_grace", Duration::from_secs(2);
    /// How long a client has after `host/hello` to send `initialize` (design
    /// 5.3.11, the ruling on T030's report).
    GATEWAY_HANDSHAKE = "gateway_handshake", Duration::from_secs(10);
    /// The wait after a failed `accept`, out of fds for one, before the next.
    GATEWAY_ACCEPT_BACKOFF = "gateway_accept_backoff", Duration::from_millis(50);
    /// How long a stream stays in memory with no handle, writer or subscriber
    /// left (design 5.2.8). Checked whenever a stream is opened, not by a timer.
    STREAM_IDLE = "stream_idle", Duration::from_secs(600);
    /// How long a new turn waits for its start snapshot before it sends the
    /// prompt anyway (design 5.2.6).
    TURN_PREPARE = "turn_prepare", Duration::from_secs(3);
    /// After an interrupt, how long the host waits for the prompt to return
    /// before it ends the turn as `host_timeout` (design 5.2.6, 5.3.9).
    INTERRUPT_FALLBACK = "interrupt_fallback", Duration::from_millis(V1.interrupt_fallback_ms);
    /// How long an item's later text fragments wait to be written as one
    /// delta (design 5.2.16, ADR-06).
    ITEM_COALESCE = "item_coalesce", Duration::from_millis(40);
    /// How long an autonomous turn may go without updates, and with no tool
    /// running, before the host ends it (design 5.2.6, Q10).
    AUTONOMOUS_SILENCE = "autonomous_silence", Duration::from_secs(10);
    /// After an autonomous turn ended on silence, how long the agent must
    /// stay silent before the queue moves on (design 5.2.6, K1).
    DEQUEUE_GUARD = "dequeue_guard", Duration::from_secs(30);
    /// How long `raw.db` keeps a frame (design 5.4.5, ADR-06).
    RAW_FRAMES_MAX_AGE = "raw_frames_max_age", Duration::from_secs(14 * DAY_SECS);
    /// How often retention prunes `raw.db` (design 5.4.5).
    RAW_FRAMES_PERIOD = "raw_frames_period", Duration::from_secs(3600);
    /// How long `audit` keeps a record (design 5.4.5).
    AUDIT_MAX_AGE = "audit_max_age", Duration::from_secs(90 * DAY_SECS);
    /// How often retention prunes `audit` (design 5.4.5).
    AUDIT_PERIOD = "audit_period", Duration::from_secs(DAY_SECS);
    /// How long after its last put a blob nothing references is kept (design
    /// 5.4.5).
    ORPHAN_BLOB_GRACE = "orphan_blob_grace", Duration::from_secs(DAY_SECS);
    /// How long the command ledger keeps a command after it began (design
    /// 5.2.5, 5.4.5).
    LEDGER_RETENTION = "ledger_retention", Duration::from_secs(48 * 60 * 60);
    /// How often the ledger drops the commands past their retention (design
    /// 5.4.5).
    LEDGER_SWEEP = "ledger_sweep", Duration::from_secs(60 * 60);
    /// How long the login shell may take to print its environment (design
    /// 5.6.6, Q18).
    LOGIN_SHELL = "login_shell", Duration::from_secs(5);
    /// How long `--version` of a CLI, git or git-lfs, and `xcode-select -p`,
    /// may take (design 5.6.6, 5.2.10).
    VERSION_PROBE = "version_probe", Duration::from_secs(5);
    /// How long the runtime installer may take to download one npm package
    /// (design 5.2.12, T115 ruling).
    RUNTIME_DOWNLOAD = "runtime_download", Duration::from_secs(60);
    /// A git command other than those below: the read-only ones and the repo
    /// tier's writes (design 5.2.10).
    GIT_COMMAND = "git_command", Duration::from_secs(30);
    /// A turn snapshot in the snapshot tier (design 5.2.10).
    GIT_SNAPSHOT = "git_snapshot", Duration::from_secs(60);
    /// `git worktree add`, which checks out the whole tree (design 5.2.10).
    GIT_WORKTREE_ADD = "git_worktree_add", Duration::from_secs(600);
    /// `git push` in the user tier (design 5.2.10).
    GIT_PUSH = "git_push", Duration::from_secs(300);
    /// The registry's own tests: a short coalescing window.
    #[cfg(test)]
    TEST_TICK = "test_tick", Duration::from_millis(40);
    /// The registry's own tests: a long idle timeout.
    #[cfg(test)]
    TEST_IDLE = "test_idle", Duration::from_secs(600);
}

/// The durations in effect. One value is shared by the whole host.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Durations {
    #[cfg(feature = "test-harness")]
    overrides: BTreeMap<&'static str, Duration>,
}

impl Durations {
    /// The duration for `key`: the test's override, if there is one, or else
    /// the design value.
    pub fn get(&self, key: DurationKey) -> Duration {
        #[cfg(feature = "test-harness")]
        if let Some(value) = self.overrides.get(key.name) {
            return *value;
        }
        key.design_value
    }
}

/// `test_config.durations` is `{"<name>_ms": <milliseconds>, ...}`; a name that
/// is not registered is an error, so a misspelt override cannot go unnoticed.
#[cfg(feature = "test-harness")]
impl<'de> serde::Deserialize<'de> for Durations {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;

        let fields = BTreeMap::<String, u64>::deserialize(deserializer)?;
        let mut overrides = BTreeMap::new();
        for (field, ms) in fields {
            let key = field
                .strip_suffix("_ms")
                .and_then(|name| ALL.iter().find(|key| key.name == name))
                .ok_or_else(|| {
                    D::Error::custom(format!(
                        "unknown duration `{field}`: expected `<name>_ms` with a registered name"
                    ))
                })?;
            overrides.insert(key.name, Duration::from_millis(ms));
        }
        Ok(Self { overrides })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn durations_registry_names_are_unique_snake_case() {
        let mut seen = HashSet::new();
        for key in ALL {
            assert!(
                seen.insert(key.name()),
                "`{}` is registered twice",
                key.name()
            );
            assert!(
                key.name()
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                "`{}` is not snake_case",
                key.name()
            );
        }
    }

    #[test]
    fn durations_default_to_their_design_values() {
        let durations = Durations::default();
        for key in ALL {
            assert_eq!(durations.get(*key), key.design_value());
        }
        assert_eq!(durations.get(TEST_TICK), Duration::from_millis(40));
    }

    #[cfg(feature = "test-harness")]
    #[test]
    fn unit_startup_payload_durations_override_registered_names_in_ms() {
        let durations: Durations = serde_json::from_str(r#"{"test_tick_ms": 5}"#).unwrap();
        assert_eq!(durations.get(TEST_TICK), Duration::from_millis(5));
        assert_eq!(durations.get(TEST_IDLE), TEST_IDLE.design_value());

        for bad in [
            r#"{"test_tick": 5}"#,
            r#"{"no_such_timer_ms": 5}"#,
            r#"{"test_tick_ms": -1}"#,
            r#"{"test_tick_ms": "5"}"#,
            r#"[]"#,
        ] {
            assert!(serde_json::from_str::<Durations>(bad).is_err(), "{bad}");
        }
    }
}
