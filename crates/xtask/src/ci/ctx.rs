//! What a job sees while it runs: the work tree, a log file per step, and the
//! tally of checks and test cases that ends up in the summary.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::clock::Stopwatch;

/// Lines of a failed step's log echoed to the terminal.
const TAIL_LINES: usize = 40;

pub struct JobCtx<'a> {
    pub root: &'a Path,
    log_dir: &'a Path,
    job: &'static str,
    /// Checks and test cases that passed or failed. A step without test cases
    /// (fmt, clippy, cargo deny, ...) counts as one check.
    pub passed: u64,
    pub failed: u64,
    pub cases: Vec<TestCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    pub name: String,
    pub passed: bool,
}

impl<'a> JobCtx<'a> {
    pub fn new(root: &'a Path, log_dir: &'a Path, job: &'static str) -> Self {
        Self {
            root,
            log_dir,
            job,
            passed: 0,
            failed: 0,
            cases: Vec::new(),
        }
    }

    /// A command that runs in the work tree root, with no stdin and no colors.
    pub fn command(&self, program: impl AsRef<OsStr>) -> Command {
        let mut cmd = Command::new(program);
        cmd.current_dir(self.root)
            .stdin(Stdio::null())
            .env("CARGO_TERM_COLOR", "never")
            .env("NO_COLOR", "1");
        cmd
    }

    /// `cargo` of the toolchain that runs xtask.
    pub fn cargo(&self) -> Command {
        self.command(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
    }

    /// Runs a step that passes or fails as a whole.
    pub fn check(&mut self, step: &str, cmd: &mut Command) -> Result<()> {
        let (ok, log) = self.exec(step, cmd)?;
        if ok {
            self.passed += 1;
            Ok(())
        } else {
            self.failed += 1;
            bail!("`{step}` failed (log: {})", self.shown(&log))
        }
    }

    /// Runs a libtest-based step (`cargo test`) and records every test case.
    pub fn libtest(&mut self, step: &str, cmd: &mut Command) -> Result<()> {
        let (ok, log) = self.exec(step, cmd)?;
        let output = fs::read_to_string(&log).context("read the step log")?;
        self.tally(step, ok, &log, parse_libtest(&output))
    }

    /// Runs a Vitest step and records every test case. Vitest writes a JSON
    /// report next to the step log; the reporter options go at the end of
    /// `cmd`, so a package's `test` script passes them on to Vitest.
    pub fn vitest(&mut self, step: &str, cmd: &mut Command) -> Result<()> {
        let report = self.step_file(step, "json");
        let mut output_file = OsString::from("--outputFile.json=");
        output_file.push(&report);
        cmd.args(["--reporter=default", "--reporter=json"])
            .arg(output_file);
        let (ok, log) = self.exec(step, cmd)?;
        let cases = match fs::read_to_string(&report) {
            Ok(text) => parse_vitest(&text, self.root),
            Err(err) => Err(err).context("Vitest wrote no JSON report"),
        };
        match cases {
            Ok(cases) => self.tally(step, ok, &log, cases),
            Err(err) => {
                self.failed += 1;
                Err(err.context(format!("`{step}` failed (log: {})", self.shown(&log))))
            }
        }
    }

    /// Adds a test step's cases to the tally; a failed step that has no failed
    /// case (it did not compile, or failed outside its tests) counts as one.
    fn tally(&mut self, step: &str, ok: bool, log: &Path, cases: Vec<TestCase>) -> Result<()> {
        let failed = cases.iter().filter(|case| !case.passed).count() as u64;
        self.passed += cases.len() as u64 - failed;
        self.failed += failed;
        self.cases.extend(cases);
        if ok {
            return Ok(());
        }
        if failed == 0 {
            self.failed += 1;
        }
        bail!("`{step}` failed (log: {})", self.shown(log))
    }

    /// `<log dir>/<job>-<step>.<extension>`.
    fn step_file(&self, step: &str, extension: &str) -> PathBuf {
        self.log_dir
            .join(format!("{}-{step}.{extension}", self.job))
    }

    fn exec(&self, step: &str, cmd: &mut Command) -> Result<(bool, PathBuf)> {
        let log_path = self.step_file(step, "log");
        let mut log = File::create(&log_path).context("create the step log")?;
        writeln!(log, "$ {cmd:?}")?;
        eprint!("  {step:<12}");
        let watch = Stopwatch::start();
        let status = cmd
            .stdout(log.try_clone()?)
            .stderr(log)
            .status()
            .with_context(|| format!("cannot start `{step}`"))?;
        let secs = watch.elapsed_ms() as f64 / 1000.0;
        eprintln!(
            "{} ({secs:.1}s)",
            if status.success() { "ok" } else { "FAILED" }
        );
        if !status.success() {
            let text = fs::read_to_string(&log_path).unwrap_or_default();
            let lines: Vec<&str> = text.lines().collect();
            for line in &lines[lines.len().saturating_sub(TAIL_LINES)..] {
                eprintln!("    | {line}");
            }
        }
        Ok((status.success(), log_path))
    }

    /// `path` relative to the work tree, for messages.
    fn shown(&self, path: &Path) -> String {
        path.strip_prefix(self.root)
            .unwrap_or(path)
            .display()
            .to_string()
    }
}

/// Test cases from libtest's default output: `test <name> ... ok|FAILED|ignored`.
/// Ignored tests are left out: they did not run.
pub fn parse_libtest(output: &str) -> Vec<TestCase> {
    output
        .lines()
        .filter_map(|line| {
            let (name, outcome) = line.strip_prefix("test ")?.rsplit_once(" ... ")?;
            let passed = match outcome {
                "ok" => true,
                "FAILED" => false,
                _ => return None,
            };
            let name = name.strip_suffix(" - should panic").unwrap_or(name);
            Some(TestCase {
                name: name.to_owned(),
                passed,
            })
        })
        .collect()
}

/// Test cases from Vitest's JSON report, named `<file>::<describe>...::<test>`
/// with the file relative to `root`. Skipped and todo tests are left out: they
/// did not run. A file that failed outside its tests (it did not load, or a
/// hook threw) counts as one failed case named after the file.
pub fn parse_vitest(report: &str, root: &Path) -> Result<Vec<TestCase>> {
    let report: Value = serde_json::from_str(report).context("parse the Vitest JSON report")?;
    let files = report["testResults"]
        .as_array()
        .context("the Vitest JSON report has no `testResults`")?;
    let mut cases = Vec::new();
    for file in files {
        let path = Path::new(file["name"].as_str().unwrap_or_default());
        let path = path
            .strip_prefix(root)
            .unwrap_or(path)
            .display()
            .to_string();
        let mut file_failed = file["status"] == "failed";
        for test in file["assertionResults"].as_array().into_iter().flatten() {
            let passed = match test["status"].as_str() {
                Some("passed") => true,
                Some("failed") => false,
                _ => continue,
            };
            file_failed &= passed;
            let titles = test["ancestorTitles"].as_array().into_iter().flatten();
            let name: Vec<&str> = std::iter::once(path.as_str())
                .chain(titles.filter_map(Value::as_str))
                .chain(test["title"].as_str())
                .collect();
            cases.push(TestCase {
                name: name.join("::"),
                passed,
            });
        }
        if file_failed {
            cases.push(TestCase {
                name: path,
                passed: false,
            });
        }
    }
    Ok(cases)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libtest_output_yields_cases_that_ran() {
        let output = "\
running 4 tests
test ci::lock::tests::unit_ci_lock_a ... ok
test ci::tests::panics - should panic ... ok
test ci::tests::broken ... FAILED
test ci::tests::slow ... ignored, needs a network
test result: FAILED. 2 passed; 1 failed; 1 ignored; 0 measured; 0 filtered out
";
        let case = |name: &str, passed| TestCase {
            name: name.to_owned(),
            passed,
        };
        assert_eq!(
            parse_libtest(output),
            [
                case("ci::lock::tests::unit_ci_lock_a", true),
                case("ci::tests::panics", true),
                case("ci::tests::broken", false),
            ]
        );
    }

    #[test]
    fn vitest_report_yields_cases_that_ran() {
        // Shaped like Vitest 5's report: a file with a failed test, a skipped
        // and a todo one; a file that did not load; a file whose hook threw.
        let report = r#"{
          "numTotalTests": 6,
          "success": false,
          "testResults": [
            {
              "name": "/work/cupper/packages/a/src/a.test.ts",
              "status": "failed",
              "message": "",
              "assertionResults": [
                {"ancestorTitles": ["UNIT-a"], "title": "passes", "status": "passed"},
                {"ancestorTitles": ["UNIT-a"], "title": "fails", "status": "failed"},
                {"ancestorTitles": ["UNIT-a"], "title": "skipped", "status": "skipped"},
                {"ancestorTitles": ["UNIT-a"], "title": "todo", "status": "todo"},
                {"ancestorTitles": [], "title": "top level", "status": "passed"}
              ]
            },
            {
              "name": "/work/cupper/packages/a/src/b.test.ts",
              "status": "failed",
              "message": "Cannot find module './missing'",
              "assertionResults": []
            },
            {
              "name": "/work/cupper/packages/a/src/c.test.ts",
              "status": "failed",
              "message": "hook",
              "assertionResults": [
                {"ancestorTitles": [], "title": "ok", "status": "passed"}
              ]
            }
          ]
        }"#;
        let case = |name: &str, passed| TestCase {
            name: name.to_owned(),
            passed,
        };
        let cases = parse_vitest(report, Path::new("/work/cupper")).expect("valid report");
        assert_eq!(
            cases,
            [
                case("packages/a/src/a.test.ts::UNIT-a::passes", true),
                case("packages/a/src/a.test.ts::UNIT-a::fails", false),
                case("packages/a/src/a.test.ts::top level", true),
                case("packages/a/src/b.test.ts", false),
                case("packages/a/src/c.test.ts::ok", true),
                case("packages/a/src/c.test.ts", false),
            ]
        );
        assert!(parse_vitest("{}", Path::new("/")).is_err());
        assert!(parse_vitest("not json", Path::new("/")).is_err());
    }
}
