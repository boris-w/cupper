//! `cargo xtask ci <job>`: the single CI entry point (plan 1.8). Jobs run on the
//! author's machine, only in a clean work tree; raw logs go to the git-ignored
//! `.private/ci-logs/`.

mod ctx;
mod git;
mod jobs;
mod lock;

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use crate::clock::{self, Stopwatch};
use ctx::{JobCtx, TestCase};
use jobs::Job;
use lock::MachineLock;

pub fn usage() -> String {
    let jobs: Vec<&str> = jobs::JOBS.iter().map(|job| job.name).collect();
    let composites: Vec<&str> = jobs::COMPOSITES.iter().map(|(name, _)| *name).collect();
    format!(
        "usage: cargo xtask ci <job>\n\njobs: {}\ncomposites: {}",
        jobs.join(", "),
        composites.join(", ")
    )
}

/// Exit code 0: every job passed; 1: a job failed; 2: the run was refused.
pub fn main(args: &[String]) -> ExitCode {
    let outcome = std::env::current_dir()
        .context("no current directory")
        .and_then(|cwd| run(args, &cwd));
    match outcome {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("xtask ci: {err:#}");
            ExitCode::from(2)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Pass,
    Fail,
    NotImplemented,
}

struct JobReport {
    name: &'static str,
    outcome: Outcome,
    passed: u64,
    failed: u64,
    duration_ms: u64,
    cases: Vec<TestCase>,
}

fn run(args: &[String], cwd: &Path) -> Result<bool> {
    let target = match args {
        [target] if !target.starts_with('-') => target.as_str(),
        _ => bail!("{}", usage()),
    };
    let selected =
        jobs::resolve(target).with_context(|| format!("unknown job `{target}`\n\n{}", usage()))?;
    if let [job] = selected.as_slice()
        && job.run.is_none()
    {
        bail!("job `{}` is registered but not implemented yet", job.name);
    }

    let root = git::toplevel(cwd)?;
    git::ensure_clean(&root)?;
    let commit = git::head(&root)?;
    let short = &commit[..12];
    let run_id = format!("{target}-{short}-{}", clock::unix_ms());
    let log_dir = root.join(".private").join("ci-logs").join(&run_id);
    fs::create_dir_all(&log_dir).context("create the log directory")?;
    eprintln!("xtask ci {target} at {short}; raw logs in .private/ci-logs/{run_id}/");

    let reports: Vec<JobReport> = selected
        .iter()
        .map(|job| run_job(job, &root, &log_dir))
        .collect();
    print_reports(&reports);
    Ok(reports.iter().all(|report| report.outcome != Outcome::Fail))
}

fn run_job(job: &Job, root: &Path, log_dir: &Path) -> JobReport {
    let mut report = JobReport {
        name: job.name,
        outcome: Outcome::NotImplemented,
        passed: 0,
        failed: 0,
        duration_ms: 0,
        cases: Vec::new(),
    };
    let Some(run) = job.run else {
        eprintln!("{}: not implemented yet, skipped", job.name);
        return report;
    };

    eprintln!("{}", job.name);
    let watch = Stopwatch::start();
    let mut ctx = JobCtx::new(root, log_dir, job.name);
    let result = machine_lock(job).and_then(|_lock| run(&mut ctx));
    if let Err(err) = &result {
        eprintln!("  {err:#}");
    }
    report.outcome = if result.is_ok() {
        Outcome::Pass
    } else {
        Outcome::Fail
    };
    report.passed = ctx.passed;
    report.failed = ctx.failed;
    report.duration_ms = watch.elapsed_ms();
    report.cases = ctx.cases;
    report
}

/// Taken for contention-sensitive jobs; released when the job ends or dies.
fn machine_lock(job: &Job) -> Result<Option<MachineLock>> {
    if !job.machine_lock {
        return Ok(None);
    }
    let path = MachineLock::default_path();
    MachineLock::acquire(&path, || {
        eprintln!("  waiting for the machine lock held by another run");
    })
    .map(Some)
    .with_context(|| format!("cannot take the machine lock {}", path.display()))
}

fn print_reports(reports: &[JobReport]) {
    eprintln!("\nresult");
    for report in reports {
        match report.outcome {
            Outcome::NotImplemented => eprintln!("  {:<18} not implemented", report.name),
            outcome => eprintln!(
                "  {:<18} {:<5} {} passed, {} failed ({:.1}s)",
                report.name,
                if outcome == Outcome::Pass {
                    "pass"
                } else {
                    "FAIL"
                },
                report.passed,
                report.failed,
                report.duration_ms as f64 / 1000.0
            ),
        }
    }
    for report in reports {
        for case in report.cases.iter().filter(|case| !case.passed) {
            eprintln!("  failed test: {} / {}", report.name, case.name);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::clean_repo;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn unit_ci_lock_refuses_to_run_in_a_dirty_work_tree() {
        let repo = clean_repo("ci-dirty");
        fs::write(repo.path().join("scratch.txt"), "uncommitted\n").expect("write");

        let err = run(&args(&["license"]), repo.path()).expect_err("dirty tree");
        assert!(err.to_string().contains("not clean"), "{err}");
        assert!(
            !repo.path().join(".private").exists(),
            "a refused run must not start any job"
        );
    }

    #[test]
    fn unimplemented_and_unknown_jobs_are_refused_up_front() {
        let repo = clean_repo("ci-refused");
        let err = run(&args(&["e2e"]), repo.path()).expect_err("not implemented");
        assert!(err.to_string().contains("not implemented"), "{err}");
        let err = run(&args(&["deploy"]), repo.path()).expect_err("unknown");
        assert!(err.to_string().contains("unknown job"), "{err}");
        assert!(!repo.path().join(".private").exists());
    }
}
