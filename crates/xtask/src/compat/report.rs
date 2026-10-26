//! The compat report (design 5.9.5): one run of `cargo xtask compat`,
//! machine-readable. Its JSON schema is generated from these types and kept
//! in `testdata/compat/report.schema.json`. A report conforms when it reads
//! into them strictly and [`problems`] finds nothing, and the schema file is
//! what they generate.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The largest integer the protocol's JSON carries exactly.
const MAX_SAFE: u64 = (1 << 53) - 1;

/// The schema file, relative to the work tree.
pub const SCHEMA: &str = "testdata/compat/report.schema.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// The format's version: 1.
    pub v: u32,
    /// The cupper commit that ran.
    pub commit: String,
    pub mode: Mode,
    pub agent: String,
    /// The model the scenarios ran on; none for the fake agent.
    pub model: Option<String>,
    pub adapter: Named,
    pub cli: Named,
    pub duration_ms: u64,
    pub scenarios: Vec<Scenario>,
    /// The real configuration files, hashed before and after the run.
    pub config_files: Vec<ConfigFile>,
    /// The CLI's sessions the run left behind, to clean up.
    pub cli_sessions: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// The fake agent plays the CLI.
    Fake,
    /// The real CLI with the user's own configuration (U-R4).
    RealConfig,
    /// The real CLI with a configuration directory of its own (after Q30).
    IsolatedConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Named {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub id: String,
    pub title: String,
    pub outcome: Outcome,
    /// Why it did not pass.
    pub detail: Option<String>,
    /// Plays, including retries while it was inconclusive.
    pub attempts: u32,
    pub duration_ms: u64,
    pub checks: Vec<Check>,
    /// Its recording is a GOLD-B sample.
    pub gold_b: bool,
    /// The recording, relative to the report.
    pub recording: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Pass,
    Fail,
    /// The agent did not do what the scenario needs.
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub check: String,
    pub passed: bool,
}

/// One file's SHA-256 before and after the run; none when it did not exist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ConfigFile {
    /// Under the home directory, as `~/...`.
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// What [`SCHEMA`] holds: the schema of [`Report`], keys sorted, since
/// serde_json keeps their order of insertion when another crate of the
/// workspace turns on `preserve_order`.
pub fn schema() -> String {
    fn sorted(value: Value) -> Value {
        match value {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.into_iter().collect();
                entries.sort_by(|a, b| a.0.cmp(&b.0));
                Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            Value::Array(items) => Value::Array(items.into_iter().map(sorted).collect()),
            other => other,
        }
    }
    let schema = sorted(schemars::schema_for!(Report).to_value());
    serde_json::to_string_pretty(&schema).expect("a schema serializes") + "\n"
}

/// A report read strictly from its JSON.
pub fn parse(text: &str) -> serde_json::Result<Report> {
    serde_json::from_str(text)
}

/// What the types cannot say about a report.
pub fn problems(report: &Report) -> Vec<String> {
    let mut found = Vec::new();
    let mut fail = |problem: String| found.push(problem);
    if report.v != 1 {
        fail(format!("version {}, not 1", report.v));
    }
    let hex = report
        .commit
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
    if report.commit.len() != 40 || !hex {
        fail("the commit is not a full hash".into());
    }
    let fake = report.mode == Mode::Fake;
    if fake && !(report.model.is_none() && report.config_files.is_empty()) {
        fail("a fake run has neither a model nor configuration files".into());
    }
    if !fake && report.config_files.is_empty() {
        fail("a real run hashes the configuration files".into());
    }
    let mut ids = Vec::new();
    for s in &report.scenarios {
        let id = &s.id;
        if id.is_empty() || ids.contains(&id) {
            fail(format!("scenario `{id}`: empty or repeated"));
        }
        ids.push(id);
        if s.attempts == 0 || s.duration_ms > MAX_SAFE {
            fail(format!("scenario {id}: attempts or duration out of range"));
        }
        let passed = s.checks.iter().all(|check| check.passed);
        if (s.outcome == Outcome::Pass) != (passed && s.detail.is_none()) {
            fail(format!("scenario {id}: outcome and checks disagree"));
        }
    }
    if report.duration_ms > MAX_SAFE {
        fail("the duration is out of range".into());
    }
    found
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn report() -> Report {
        let scenario = |id: &str, passed: bool| {
            json!({"id": id, "title": id, "outcome": if passed { "pass" } else { "fail" },
                "detail": (!passed).then_some("not met: x"), "attempts": 1, "duration_ms": 150,
                "checks": [{"check": "x", "passed": passed}], "gold_b": false, "recording": null})
        };
        let report = json!({"v": 1, "commit": "0123456789abcdef0123456789abcdef01234567",
            "mode": "fake", "agent": "claude", "model": null, "duration_ms": 4800,
            "adapter": {"name": "cupper-fake-agent", "version": "0.0.0"},
            "cli": {"name": "claude", "version": "fake"},
            "scenarios": [scenario("approval", true), scenario("reject", false)],
            "config_files": [], "cli_sessions": []});
        parse(&report.to_string()).unwrap()
    }

    #[test]
    fn unit_compat_report_round_trips_and_conforms() {
        let report = report();
        let text = serde_json::to_string(&report).unwrap();
        assert_eq!(parse(&text).unwrap(), report);
        assert_eq!(problems(&report), Vec::<String>::new());
    }

    #[test]
    fn unit_compat_report_reads_strictly() {
        let mut value = serde_json::to_value(report()).unwrap();
        value["scenarios"][0]["stray"] = true.into();
        assert!(parse(&value.to_string()).is_err());
        let mut value = serde_json::to_value(report()).unwrap();
        value["mode"] = "real".into();
        assert!(parse(&value.to_string()).is_err());
    }

    #[test]
    fn unit_compat_report_finds_what_the_types_cannot_say() {
        let mut bad = report();
        bad.v = 2;
        bad.commit = "HEAD".into();
        bad.model = Some("haiku".into());
        bad.scenarios[1].id = "approval".into();
        bad.scenarios[0].attempts = 0;
        bad.scenarios[0].checks[0].passed = false;
        bad.duration_ms = 1 << 53;
        assert_eq!(problems(&bad).len(), 7, "{:#?}", problems(&bad));
        let mut real = report();
        real.mode = Mode::RealConfig;
        assert_eq!(
            problems(&real),
            ["a real run hashes the configuration files"]
        );
    }

    /// `UPDATE_GOLDEN=1` writes the schema file instead.
    #[test]
    fn unit_compat_report_schema_file_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(SCHEMA);
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, schema()).unwrap();
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            committed == schema(),
            "{SCHEMA} is stale: UPDATE_GOLDEN=1 cargo test -p xtask"
        );
    }
}
