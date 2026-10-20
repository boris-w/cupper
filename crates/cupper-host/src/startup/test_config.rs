//! `test_config`: the injection points of `test-harness` builds (design 5.9.9).
//!
//! It arrives only in the stdin payload, never as a switch (plan P-9). This
//! module defines and parses it; each injection point's user wires it up.
//! Every object rejects keys it does not know, so a misspelt injection fails
//! the payload (exit 1) instead of silently testing the default.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Deserializer};

use crate::durations::Durations;
use crate::failpoint::Failpoints;

/// Every field is optional; a missing one keeps the production behaviour.
#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TestConfig {
    /// Which clock the host runs on (design 5.2.16).
    pub clock: ClockKind,
    /// Overrides of registered durations: `{"<name>_ms": n}`.
    pub durations: Durations,
    /// Armed failpoints: `[{"name": "<module>.<place>", "hit"?: n}]` (5.9.8).
    pub failpoints: Failpoints,
    /// Stands in for `kern.bootsessionuuid` when deciding on `data_epoch`
    /// (design 5.2.8, 5.9.8).
    pub boot_session_uuid: Option<String>,
    /// The program each agent runs, instead of the adapter that the runtime
    /// installer and CLI discovery would provide (the fake agent, 5.9.2).
    pub agents: PerAgent<AgentCommand>,
    pub runtime: RuntimeConfig,
    /// The login-shell environment (`PATH`, `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
    /// ...) to use instead of running the login shell.
    pub login_env: Option<BTreeMap<String, String>>,
    pub notifier: NotifierKind,
    /// Resident memory, in bytes, to report for each agent's adapter process
    /// group instead of sampling it (K20, design 5.6.8).
    pub rss_samples: PerAgent<u64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockKind {
    /// `SystemClock`.
    #[default]
    System,
    /// `ManualClock`, which moves only on `test/clock_advance`.
    Manual,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotifierKind {
    /// System notifications.
    #[default]
    System,
    /// Notifications go to the test's receiver; UserNotifications is never
    /// called, so no permission prompt takes the focus.
    Record,
}

/// One value per agent.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerAgent<T> {
    pub claude: Option<T>,
    pub codex: Option<T>,
}

impl<T> Default for PerAgent<T> {
    fn default() -> Self {
        Self {
            claude: None,
            codex: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCommand {
    /// An absolute path: `cupper_sys::spawn` does not search `PATH`.
    #[serde(deserialize_with = "absolute_path")]
    pub program: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RuntimeConfig {
    /// The npm registry to download from instead of `registry.npmjs.org`, so
    /// installer tests run offline against a local fixture.
    pub registry_url: Option<String>,
    /// A manifest to plan the runtime from instead of the one compiled in,
    /// for a bundle a test makes.
    pub manifest: Option<PathBuf>,
}

fn absolute_path<'de, D: Deserializer<'de>>(deserializer: D) -> Result<PathBuf, D::Error> {
    let path = PathBuf::deserialize(deserializer)?;
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(serde::de::Error::custom(format!(
            "{} is not an absolute path",
            path.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::durations;

    fn config(json: &str) -> Result<TestConfig, serde_json::Error> {
        serde_json::from_str(json)
    }

    #[test]
    fn unit_startup_payload_test_config_defaults_to_production() {
        let empty = config("{}").unwrap();
        assert_eq!(empty.clock, ClockKind::System);
        assert_eq!(empty.durations, Durations::default());
        assert!(empty.failpoints.triggers().is_empty());
        assert_eq!(empty.boot_session_uuid, None);
        assert_eq!(empty.agents, PerAgent::default());
        assert_eq!(empty.runtime, RuntimeConfig::default());
        assert_eq!(empty.login_env, None);
        assert_eq!(empty.notifier, NotifierKind::System);
        assert_eq!(empty.rss_samples, PerAgent::default());
    }

    #[test]
    fn unit_startup_payload_test_config_reads_every_injection_point() {
        let full = config(
            r#"{
                "clock": "manual",
                "durations": {"test_tick_ms": 1},
                "failpoints": [{"name": "test.probe", "hit": 2}],
                "boot_session_uuid": "8D4E1A0C-0000-4000-8000-000000000001",
                "agents": {"claude": {"program": "/opt/fake-agent", "args": ["--persona", "claude"]}},
                "runtime": {"registry_url": "http://127.0.0.1:4873/"},
                "login_env": {"PATH": "/usr/bin:/bin"},
                "notifier": "record",
                "rss_samples": {"codex": 3221225472}
            }"#,
        )
        .unwrap();
        assert_eq!(full.clock, ClockKind::Manual);
        assert_eq!(
            full.durations.get(durations::TEST_TICK),
            std::time::Duration::from_millis(1)
        );
        assert_eq!(full.failpoints.triggers().len(), 1);
        assert_eq!(
            full.boot_session_uuid.as_deref(),
            Some("8D4E1A0C-0000-4000-8000-000000000001")
        );
        let claude = full.agents.claude.as_ref().unwrap();
        assert_eq!(claude.program, PathBuf::from("/opt/fake-agent"));
        assert_eq!(claude.args, ["--persona", "claude"]);
        assert_eq!(full.agents.codex, None);
        assert_eq!(
            full.runtime.registry_url.as_deref(),
            Some("http://127.0.0.1:4873/")
        );
        assert_eq!(full.login_env.unwrap()["PATH"], "/usr/bin:/bin");
        assert_eq!(full.notifier, NotifierKind::Record);
        assert_eq!(full.rss_samples.codex, Some(3_221_225_472));
        assert_eq!(full.rss_samples.claude, None);
    }

    #[test]
    fn unit_startup_payload_test_config_rejects_what_it_does_not_know() {
        for bad in [
            r#"{"clocks": "manual"}"#,
            r#"{"clock": "virtual"}"#,
            r#"{"durations": {"no_such_timer_ms": 1}}"#,
            r#"{"failpoints": [{"name": "no.such_failpoint"}]}"#,
            r#"{"agents": {"gemini": {"program": "/opt/fake-agent"}}}"#,
            r#"{"agents": {"claude": {"program": "fake-agent"}}}"#,
            r#"{"agents": {"claude": {"program": "/opt/fake-agent", "env": {}}}}"#,
            r#"{"runtime": {"registry": "http://127.0.0.1/"}}"#,
            r#"{"notifier": "silent"}"#,
            r#"{"rss_samples": {"codex": -1}}"#,
        ] {
            assert!(config(bad).is_err(), "{bad}");
        }
    }
}
