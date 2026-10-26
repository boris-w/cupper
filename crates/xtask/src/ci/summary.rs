//! The structured summary of a CI run: the only CI output that is committed
//! (plan 1.8). It holds allowlisted fields only, and is checked before it is
//! written: anything that looks like an absolute path, a user, host or git
//! author name, or an email address makes `write` refuse.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};

use super::{JobReport, Outcome, ids};

const FORMAT_VERSION: u64 = 1;

/// The allowlist: objects may carry only the listed keys.
enum Shape {
    Scalar,
    Object(&'static [(&'static str, Shape)]),
    List(&'static Shape),
}

const SUMMARY: Shape = Shape::Object(&[
    ("version", Shape::Scalar),
    ("commit", Shape::Scalar),
    ("target", Shape::Scalar),
    (
        "toolchain",
        Shape::Object(&[
            ("rustc", Shape::Scalar),
            ("cargo", Shape::Scalar),
            ("cargo_deny", Shape::Scalar),
            ("node", Shape::Scalar),
            ("pnpm", Shape::Scalar),
        ]),
    ),
    ("os", Shape::Scalar),
    ("machine", Shape::Scalar),
    ("duration_ms", Shape::Scalar),
    (
        "jobs",
        Shape::List(&Shape::Object(&[
            ("name", Shape::Scalar),
            ("result", Shape::Scalar),
            ("passed", Shape::Scalar),
            ("failed", Shape::Scalar),
            ("duration_ms", Shape::Scalar),
            ("skipped", Shape::List(&Shape::Scalar)),
        ])),
    ),
    (
        "results",
        Shape::List(&Shape::Object(&[
            ("id", Shape::Scalar),
            ("suite", Shape::Scalar),
            ("result", Shape::Scalar),
            ("commit", Shape::Scalar),
        ])),
    ),
    (
        "artifacts",
        Shape::List(&Shape::Object(&[
            ("name", Shape::Scalar),
            ("sha256", Shape::Scalar),
        ])),
    ),
]);

pub struct Run<'a> {
    pub commit: &'a str,
    /// The job or composite that was invoked.
    pub target: &'a str,
    /// The suite of every result: the composite the job ran within (`--in`),
    /// or the target.
    pub suite: &'a str,
    pub toolchain: &'a [(&'static str, Option<String>)],
    pub os: &'a str,
    pub machine: &'a str,
    pub duration_ms: u64,
    pub reports: &'a [JobReport],
    pub ids: &'a [String],
}

pub fn build(run: &Run) -> Value {
    let toolchain: Map<String, Value> = run
        .toolchain
        .iter()
        .map(|(tool, version)| ((*tool).to_owned(), json!(version)))
        .collect();
    let jobs: Vec<Value> = run
        .reports
        .iter()
        .map(|report| {
            json!({
                "name": report.name,
                "result": report.outcome.as_str(),
                "passed": report.passed,
                "failed": report.failed,
                "duration_ms": report.duration_ms,
                "skipped": report.skipped,
            })
        })
        .collect();
    json!({
        "version": FORMAT_VERSION,
        "commit": run.commit,
        "target": run.target,
        "toolchain": toolchain,
        "os": run.os,
        "machine": run.machine,
        "duration_ms": run.duration_ms,
        "jobs": jobs,
        "results": results(run),
        "artifacts": run
            .reports
            .iter()
            .flat_map(|report| &report.artifacts)
            .map(|artifact| json!({"name": artifact.name, "sha256": artifact.sha256}))
            .collect::<Vec<_>>(),
    })
}

/// One entry per test ID that ran: it passes when all of its tests passed.
fn results(run: &Run) -> Vec<Value> {
    let mut passed_by_id: BTreeMap<&str, bool> = BTreeMap::new();
    for case in run.reports.iter().flat_map(|report| &report.cases) {
        if let Some(id) = ids::owner(run.ids, &case.name) {
            *passed_by_id.entry(id).or_insert(true) &= case.passed;
        }
    }
    passed_by_id
        .into_iter()
        .map(|(id, passed)| {
            json!({
                "id": id,
                "suite": run.suite,
                "result": if passed { Outcome::Pass } else { Outcome::Fail }.as_str(),
                "commit": run.commit,
            })
        })
        .collect()
}

/// Checks the summary, then writes it as `<dir>/<file_name>`.
pub fn write(dir: &Path, file_name: &str, summary: &Value, identity: &[String]) -> Result<PathBuf> {
    check_shape(summary, &SUMMARY, "summary")
        .map_err(|err| anyhow!("refusing to write the summary: {err}"))?;
    let problems = personal_data(summary, identity);
    if !problems.is_empty() {
        bail!(
            "refusing to write the summary; it contains:\n  {}",
            problems.join("\n  ")
        );
    }
    fs::create_dir_all(dir).context("create the record directory")?;
    let path = dir.join(file_name);
    let text = serde_json::to_string_pretty(summary)? + "\n";
    fs::write(&path, text).context("write the summary")?;
    Ok(path)
}

fn check_shape(value: &Value, shape: &Shape, at: &str) -> Result<(), String> {
    match (shape, value) {
        (Shape::Scalar, Value::Object(_) | Value::Array(_)) => {
            Err(format!("`{at}` must be a plain value"))
        }
        (Shape::Scalar, _) => Ok(()),
        (Shape::List(item), Value::Array(items)) => items
            .iter()
            .try_for_each(|value| check_shape(value, item, &format!("{at}[]"))),
        (Shape::Object(fields), Value::Object(map)) => map.iter().try_for_each(|(key, value)| {
            let (_, inner) = fields
                .iter()
                .find(|(name, _)| name == key)
                .ok_or_else(|| format!("`{at}.{key}` is not an allowlisted field"))?;
            check_shape(value, inner, &format!("{at}.{key}"))
        }),
        _ => Err(format!("`{at}` has the wrong type")),
    }
}

/// Every string value that looks personal, with the reason.
fn personal_data(value: &Value, identity: &[String]) -> Vec<String> {
    match value {
        Value::String(text) => personal(text, identity)
            .map(|reason| format!("{reason}: {text:?}"))
            .into_iter()
            .collect(),
        Value::Array(items) => items
            .iter()
            .flat_map(|item| personal_data(item, identity))
            .collect(),
        Value::Object(map) => map
            .values()
            .flat_map(|item| personal_data(item, identity))
            .collect(),
        _ => Vec::new(),
    }
}

fn personal(text: &str, identity: &[String]) -> Option<&'static str> {
    let lower = text.to_lowercase();
    let tokens = || {
        lower
            .split(|c: char| c.is_whitespace() || "\"'`()[]{}<>,;:=".contains(c))
            .filter(|token| !token.is_empty())
    };
    if lower.contains(":\\")
        || lower.starts_with("\\\\")
        || tokens()
            .any(|token| token.starts_with('~') || (token.starts_with('/') && token.len() > 1))
    {
        return Some("absolute path");
    }
    if identity.iter().any(|name| contains_word(&lower, name)) {
        return Some("user, host or git author name");
    }
    let email = |token: &str| {
        token
            .split_once('@')
            .is_some_and(|(user, domain)| !user.is_empty() && domain.contains('.'))
    };
    if tokens().any(email) {
        return Some("email address");
    }
    None
}

/// `needle` occurs in `hay` with no letter or digit right before or after it.
fn contains_word(hay: &str, needle: &str) -> bool {
    hay.match_indices(needle).any(|(at, _)| {
        let before = hay[..at].chars().next_back();
        let after = hay[at + needle.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ci::ctx::{Artifact, TestCase};
    use crate::testutil::TempDir;

    const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

    fn sample() -> Value {
        let case = |name: &str, passed| TestCase {
            name: name.to_owned(),
            passed,
        };
        let reports = [
            JobReport {
                name: "rust",
                outcome: Outcome::Fail,
                passed: 3,
                failed: 1,
                duration_ms: 1200,
                cases: vec![
                    case("ci::tests::unit_ci_summary_a", true),
                    case("ci::tests::unit_ci_lock_a", true),
                    case("ci::tests::unit_ci_lock_b", false),
                    case("ci::tests::helper", true),
                ],
                skipped: vec!["sensitive-words".to_owned()],
                artifacts: Vec::new(),
            },
            JobReport {
                name: "perf",
                outcome: Outcome::NotImplemented,
                passed: 0,
                failed: 0,
                duration_ms: 0,
                cases: Vec::new(),
                skipped: Vec::new(),
                artifacts: Vec::new(),
            },
            JobReport {
                name: "e2e",
                outcome: Outcome::Pass,
                passed: 1,
                failed: 0,
                duration_ms: 9000,
                cases: Vec::new(),
                skipped: Vec::new(),
                artifacts: vec![Artifact {
                    name: "Cupper E2E.app".to_owned(),
                    sha256: "ab".repeat(32),
                }],
            },
        ];
        build(&Run {
            commit: COMMIT,
            target: "pr",
            suite: "pr",
            toolchain: &[("rustc", Some("1.96.0".to_owned())), ("pnpm", None)],
            os: "macOS 26.4",
            machine: "MacBook Pro (Apple M4 Max)",
            duration_ms: 1500,
            reports: &reports,
            ids: &["UNIT-ci-lock".to_owned(), "UNIT-ci-summary".to_owned()],
        })
    }

    #[test]
    fn unit_ci_summary_holds_allowlisted_fields_only() {
        let summary = sample();
        check_shape(&summary, &SUMMARY, "summary").expect("a built summary fits the allowlist");
        assert_eq!(
            summary["results"],
            json!([
                {"id": "UNIT-ci-lock", "suite": "pr", "result": "fail", "commit": COMMIT},
                {"id": "UNIT-ci-summary", "suite": "pr", "result": "pass", "commit": COMMIT},
            ])
        );
        assert_eq!(summary["jobs"][1]["result"], "not_implemented");
        assert_eq!(summary["jobs"][0]["skipped"], json!(["sensitive-words"]));
        assert_eq!(
            summary["artifacts"],
            json!([{"name": "Cupper E2E.app", "sha256": "ab".repeat(32)}])
        );

        for (object, extra) in [
            ("", "hostname"),
            ("/toolchain", "rustup_home"),
            ("/jobs/0", "log"),
            ("/results/0", "author"),
            ("/artifacts/0", "path"),
        ] {
            let mut bad = summary.clone();
            let map = bad.pointer_mut(object).and_then(Value::as_object_mut);
            map.expect("object").insert(extra.to_owned(), json!("x"));
            let err = check_shape(&bad, &SUMMARY, "summary").expect_err(extra);
            assert!(err.contains(extra), "{err}");
        }
        let mut nested = summary.clone();
        nested["os"] = json!({"name": "macOS"});
        check_shape(&nested, &SUMMARY, "summary").expect_err("object in a scalar field");
    }

    #[test]
    fn unit_ci_summary_refuses_paths_and_identity() {
        let dir = TempDir::new("summary");
        let identity = ["alice".to_owned(), "alice-mbp".to_owned()];
        let clean = sample();
        let path = write(dir.path(), "pr.json", &clean, &identity).expect("clean summary");
        let text = fs::read_to_string(path).expect("read back");
        assert_eq!(serde_json::from_str::<Value>(&text).expect("json"), clean);

        for leak in [
            "/Users/someone/cupper",
            "built in /tmp/ci",
            "~/src/cupper",
            "file:///opt/x",
            r"C:\Users\x",
            "Alice",
            "ALICE-MBP.local",
            "someone@example.com",
        ] {
            let mut bad = clean.clone();
            bad["machine"] = json!(leak);
            let err = write(dir.path(), "leak.json", &bad, &identity).expect_err(leak);
            assert!(err.to_string().contains("refusing"), "{err}");
            assert!(!dir.path().join("leak.json").exists(), "{leak} was written");
        }
        // A name inside another word is not a match.
        assert_eq!(personal("Malice-free 1.2", &identity), None);
    }
}
