//! Failpoints (design 5.9.8, test #9): named places in the host where a
//! `test-harness` build exits on purpose, so an integration test can check
//! what a crash at exactly that place leaves behind.
//!
//! - A failpoint is registered in the list below and marked in the code with
//!   [`failpoint!`](crate::failpoint!). The task that adds the failpoint
//!   registers it in the same change.
//! - A test arms failpoints through `test_config.failpoints`:
//!   `[{"name": "<name>", "hit"?: n}]`. The process then exits with
//!   [`HostExit::Failpoint`](crate::exit::HostExit::Failpoint) the `n`-th time
//!   (by default the first) execution reaches that failpoint. Reaching a
//!   failpoint that is not armed changes nothing.
//! - The exit runs no destructor and flushes nothing the host buffers, so it
//!   leaves the data as a crash would.
//! - Without `test-harness` the macro expands to nothing and nothing else in
//!   this module exists (design 5.9.9).

/// Marks the failpoint registered as `$point`, e.g.
/// `failpoint!(STORE_BEFORE_COMMIT)`: with `test-harness`, the process exits
/// here if a test armed it; otherwise this expands to nothing.
#[cfg(feature = "test-harness")]
#[macro_export]
macro_rules! failpoint {
    ($point:ident) => {
        $crate::failpoint::hit($crate::failpoint::$point)
    };
}

/// Marks the failpoint registered as `$point`, e.g.
/// `failpoint!(STORE_BEFORE_COMMIT)`: with `test-harness`, the process exits
/// here if a test armed it; otherwise this expands to nothing.
#[cfg(not(feature = "test-harness"))]
#[macro_export]
macro_rules! failpoint {
    ($point:ident) => {};
}

#[cfg(feature = "test-harness")]
pub use harness::*;

#[cfg(feature = "test-harness")]
mod harness {
    use std::num::NonZeroU64;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicU64, Ordering};

    use serde::de::Error as _;
    use serde::{Deserialize, Deserializer};

    use crate::exit::HostExit;

    /// A registered failpoint.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Failpoint(&'static str);

    impl Failpoint {
        pub const fn name(self) -> &'static str {
            self.0
        }

        pub fn from_name(name: &str) -> Option<Self> {
            ALL.iter().copied().find(|point| point.0 == name)
        }
    }

    macro_rules! failpoints {
        ($(
            $(#[doc = $doc:literal])*
            $(#[cfg($cfg:meta)])?
            $point:ident = $name:literal;
        )*) => {
            $(
                $(#[doc = $doc])*
                $(#[cfg($cfg)])?
                pub const $point: Failpoint = Failpoint($name);
            )*

            /// Every registered failpoint.
            pub const ALL: &[Failpoint] = &[$($(#[cfg($cfg)])? $point,)*];
        };
    }

    // Names are `<module>.<place>`, e.g. `store.before_commit`.
    failpoints! {
        /// The module's own tests.
        #[cfg(test)]
        TEST_PROBE = "test.probe";
        /// The module's own tests.
        #[cfg(test)]
        TEST_OTHER = "test.other";
    }

    /// The failpoints a test armed (`test_config.failpoints`): each at most once.
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Failpoints(Vec<Trigger>);

    /// Exit when execution reaches `point` for the `hit`-th time.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Trigger {
        pub point: Failpoint,
        pub hit: NonZeroU64,
    }

    impl Failpoints {
        pub fn triggers(&self) -> &[Trigger] {
            &self.0
        }
    }

    impl<'de> Deserialize<'de> for Failpoints {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Wire {
                name: String,
                #[serde(default = "first")]
                hit: NonZeroU64,
            }

            fn first() -> NonZeroU64 {
                NonZeroU64::MIN
            }

            let mut triggers: Vec<Trigger> = Vec::new();
            for Wire { name, hit } in Vec::<Wire>::deserialize(deserializer)? {
                let point = Failpoint::from_name(&name)
                    .ok_or_else(|| D::Error::custom(format!("unknown failpoint `{name}`")))?;
                if triggers.iter().any(|trigger| trigger.point == point) {
                    return Err(D::Error::custom(format!(
                        "failpoint `{name}` is armed twice"
                    )));
                }
                triggers.push(Trigger { point, hit });
            }
            Ok(Self(triggers))
        }
    }

    struct Armed {
        trigger: Trigger,
        hits: AtomicU64,
    }

    static ARMED: OnceLock<Vec<Armed>> = OnceLock::new();

    #[derive(Debug, thiserror::Error)]
    #[error("failpoints are already armed")]
    pub struct AlreadyArmed;

    /// Arms `failpoints` for the rest of the process. The host does this once,
    /// at startup.
    pub fn arm(failpoints: &Failpoints) -> Result<(), AlreadyArmed> {
        let armed = failpoints
            .0
            .iter()
            .map(|trigger| Armed {
                trigger: *trigger,
                hits: AtomicU64::new(0),
            })
            .collect();
        ARMED.set(armed).map_err(|_| AlreadyArmed)
    }

    /// Execution reached `point`. Call it through [`failpoint!`](crate::failpoint!).
    pub fn hit(point: Failpoint) {
        let Some(armed) = ARMED.get() else {
            return;
        };
        for entry in armed.iter().filter(|entry| entry.trigger.point == point) {
            if entry.hits.fetch_add(1, Ordering::Relaxed) + 1 == entry.trigger.hit.get() {
                fire(point);
            }
        }
    }

    fn fire(point: Failpoint) -> ! {
        eprintln!("cupper-host: failpoint `{}` fired", point.name());
        std::process::exit(HostExit::Failpoint.code().into())
    }

    #[cfg(test)]
    mod tests {
        use std::collections::HashSet;
        use std::process::Command;

        use super::*;

        /// Set in the child process that arms a failpoint and runs into it.
        const CHILD: &str = "CUPPER_TEST_FAILPOINT_CHILD";

        fn plan(json: &str) -> Result<Failpoints, serde_json::Error> {
            serde_json::from_str(json)
        }

        #[test]
        fn failpoint_registry_names_are_unique_and_dotted() {
            let mut seen = HashSet::new();
            for point in ALL {
                let name = point.name();
                assert!(seen.insert(name), "`{name}` is registered twice");
                let (module, place) = name.split_once('.').expect("`<module>.<place>`");
                for part in [module, place] {
                    assert!(
                        !part.is_empty()
                            && part
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'),
                        "`{name}` is not `<module>.<place>` in snake_case"
                    );
                }
                assert_eq!(Failpoint::from_name(name), Some(*point));
            }
        }

        #[test]
        fn unit_failpoint_plan_names_registered_failpoints_once() {
            let armed =
                plan(r#"[{"name": "test.probe"}, {"name": "test.other", "hit": 3}]"#).unwrap();
            assert_eq!(
                armed.triggers(),
                [
                    Trigger {
                        point: TEST_PROBE,
                        hit: NonZeroU64::MIN
                    },
                    Trigger {
                        point: TEST_OTHER,
                        hit: NonZeroU64::new(3).unwrap()
                    },
                ]
            );
            for bad in [
                r#"[{"name": "test.nope"}]"#,
                r#"[{"name": "test.probe"}, {"name": "test.probe", "hit": 2}]"#,
                r#"[{"name": "test.probe", "hit": 0}]"#,
                r#"[{"name": "test.probe", "when": "now"}]"#,
                r#"["test.probe"]"#,
            ] {
                assert!(plan(bad).is_err(), "{bad}");
            }
        }

        /// This process never arms a failpoint (only the child does), so the
        /// registry must not even be initialised by reaching one.
        #[test]
        fn unit_failpoint_unarmed_failpoint_has_no_effect() {
            for _ in 0..3 {
                crate::failpoint!(TEST_PROBE);
            }
            assert!(ARMED.get().is_none());
        }

        #[test]
        fn unit_failpoint_armed_failpoint_exits_on_its_nth_hit() {
            let child = concat!(module_path!(), "::failpoint_child");
            let (_crate, child) = child.split_once("::").expect("a module path");
            let out = Command::new(std::env::current_exe().expect("the test binary"))
                .args(["--exact", child, "--ignored", "--nocapture"])
                .env(CHILD, "1")
                .output()
                .expect("run the child");
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(HostExit::Failpoint.code().into()),
                "{stdout}\n{stderr}"
            );
            assert!(stdout.contains("other failpoint passed"), "{stdout}");
            assert!(stdout.contains("first hit passed"), "{stdout}");
            assert!(!stdout.contains("second hit passed"), "{stdout}");
            assert!(stderr.contains("failpoint `test.probe` fired"), "{stderr}");
        }

        #[test]
        #[ignore = "the child process of unit_failpoint_armed_failpoint_exits_on_its_nth_hit"]
        fn failpoint_child() {
            if std::env::var_os(CHILD).is_none() {
                return;
            }
            let armed = plan(r#"[{"name": "test.probe", "hit": 2}]"#).unwrap();
            arm(&armed).unwrap();
            assert!(arm(&armed).is_err(), "armed twice");
            for _ in 0..3 {
                crate::failpoint!(TEST_OTHER);
            }
            println!("other failpoint passed");
            crate::failpoint!(TEST_PROBE);
            println!("first hit passed");
            crate::failpoint!(TEST_PROBE);
            println!("second hit passed");
        }
    }
}

/// No failpoint exists without `test-harness`: the macro does not even look
/// the name up.
#[cfg(all(test, not(any(feature = "dev-auth", feature = "test-harness"))))]
mod tests {
    #[test]
    fn unit_release_features_failpoints_expand_to_nothing() {
        crate::failpoint!(NOT_A_REGISTERED_FAILPOINT);
    }
}
