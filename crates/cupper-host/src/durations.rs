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
    /// How long a new turn waits for its start snapshot before it sends the
    /// prompt anyway (design 5.2.6).
    TURN_PREPARE = "turn_prepare", Duration::from_secs(3);
    /// After an interrupt, how long the host waits for the prompt to return
    /// before it ends the turn as `host_timeout` (design 5.2.6, 5.3.9).
    INTERRUPT_FALLBACK = "interrupt_fallback", Duration::from_millis(V1.interrupt_fallback_ms);
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
