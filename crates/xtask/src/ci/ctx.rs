//! What a job sees while it runs: the work tree, a log file per step, and the
//! tally of checks and test cases that ends up in the summary.

use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

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
        let cases = parse_libtest(&output);
        let failed = cases.iter().filter(|case| !case.passed).count() as u64;
        self.passed += cases.len() as u64 - failed;
        self.failed += failed;
        self.cases.extend(cases);
        if ok {
            return Ok(());
        }
        if failed == 0 {
            // Failed before any test ran, e.g. a compile error.
            self.failed += 1;
        }
        bail!("`{step}` failed (log: {})", self.shown(&log))
    }

    fn exec(&self, step: &str, cmd: &mut Command) -> Result<(bool, PathBuf)> {
        let log_path = self.log_dir.join(format!("{}-{step}.log", self.job));
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
}
