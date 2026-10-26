//! `cargo xtask ci <job> [--record <dir>]`: the single CI entry point (plan 1.8).
//! Jobs run on the author's machine, only in a clean work tree; raw logs go to
//! the git-ignored `.private/ci-logs/`, and `--record` writes the structured
//! summary that may be committed.

mod ctx;
mod facts;
pub mod git;
mod ids;
pub mod jobs;
pub mod lock;
mod summary;

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use crate::clock::{self, Stopwatch};
use ctx::{Artifact, JobCtx, TestCase};
use jobs::Job;
use lock::MachineLock;

pub fn usage() -> String {
    let jobs: Vec<&str> = jobs::JOBS.iter().map(|job| job.name).collect();
    let composites: Vec<&str> = jobs::COMPOSITES.iter().map(|(name, _)| *name).collect();
    format!(
        "usage: cargo xtask ci <job> [--record <dir>]\n\njobs: {}\ncomposites: {}\n\n\
         --record <dir>  write the run summary to <dir>/<job>-<commit>.json,\n\
         \x20               e.g. docs/tasks/<ID>-<slug>/ci/",
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

impl Outcome {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::NotImplemented => "not_implemented",
        }
    }
}

struct JobReport {
    name: &'static str,
    outcome: Outcome,
    passed: u64,
    failed: u64,
    duration_ms: u64,
    cases: Vec<TestCase>,
    skipped: Vec<String>,
    artifacts: Vec<Artifact>,
}

struct Options<'a> {
    target: &'a str,
    record: Option<PathBuf>,
}

fn parse(args: &[String]) -> Result<Options<'_>> {
    let mut target = None;
    let mut record = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--record" => {
                let dir = rest.next().context("`--record` needs a directory")?;
                record = Some(PathBuf::from(dir));
            }
            flag if flag.starts_with('-') => bail!("unknown option `{flag}`\n\n{}", usage()),
            name if target.is_none() => target = Some(name),
            extra => bail!("unexpected argument `{extra}`\n\n{}", usage()),
        }
    }
    let target = target.with_context(usage)?;
    Ok(Options { target, record })
}

fn run(args: &[String], cwd: &Path) -> Result<bool> {
    let Options { target, record } = parse(args)?;
    refuse_golden_update(std::env::var_os("UPDATE_GOLDEN"))?;
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

    let watch = Stopwatch::start();
    let reports: Vec<JobReport> = selected
        .iter()
        .map(|job| run_job(job, target, &root, &log_dir))
        .collect();
    let duration_ms = watch.elapsed_ms();
    print_reports(&reports);

    if let Some(dir) = record {
        let machine = facts::machine(&root);
        // The generic model name is not personal even where a host is named after it.
        let mut identity = facts::identity(&root);
        identity.retain(|name| !machine.to_lowercase().contains(name.as_str()));
        let summary = summary::build(&summary::Run {
            commit: &commit,
            target,
            toolchain: &facts::toolchain(&root),
            os: &facts::os(&root),
            machine: &machine,
            duration_ms,
            reports: &reports,
            ids: &ids::harvest(&root),
        });
        let file_name = format!("{target}-{short}.json");
        let path = summary::write(&cwd.join(dir), &file_name, &summary, &identity)?;
        let shown = path.strip_prefix(&root).unwrap_or(&path);
        eprintln!("summary written to {}", shown.display());
    }
    Ok(reports.iter().all(|report| report.outcome != Outcome::Fail))
}

/// Goldens change by hand and get reviewed in the PR; CI only compares
/// (design 5.9.3).
fn refuse_golden_update(update_golden: Option<OsString>) -> Result<()> {
    if update_golden.is_some() {
        bail!("UPDATE_GOLDEN is set: goldens are updated by hand and reviewed, never in CI");
    }
    Ok(())
}

fn run_job(job: &Job, target: &str, root: &Path, log_dir: &Path) -> JobReport {
    let mut report = JobReport {
        name: job.name,
        outcome: Outcome::NotImplemented,
        passed: 0,
        failed: 0,
        duration_ms: 0,
        cases: Vec::new(),
        skipped: Vec::new(),
        artifacts: Vec::new(),
    };
    let Some(run) = job.run else {
        eprintln!("{}: not implemented yet, skipped", job.name);
        return report;
    };

    eprintln!("{}", job.name);
    let watch = Stopwatch::start();
    let mut ctx = JobCtx::new(root, log_dir, job.name);
    ctx.target = target;
    ctx.machine_lock = job.machine_lock;
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
    report.skipped = ctx.skipped;
    report.artifacts = ctx.artifacts;
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
    for report in reports.iter().filter(|report| !report.skipped.is_empty()) {
        let steps = report.skipped.join(", ");
        eprintln!("  skipped steps: {} / {steps}", report.name);
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
    fn options_take_a_target_and_an_optional_record_dir() {
        let given = args(&["pr", "--record", "docs/x/ci"]);
        let parsed = parse(&given).expect("valid");
        assert_eq!(parsed.target, "pr");
        assert_eq!(parsed.record, Some(PathBuf::from("docs/x/ci")));
        assert!(parse(&args(&["pr", "--record"])).is_err());
        assert!(parse(&args(&["pr", "rust"])).is_err());
        assert!(parse(&args(&["--verbose", "pr"])).is_err());
        assert!(parse(&args(&[])).is_err());
    }

    #[test]
    fn ci_refuses_to_update_goldens() {
        refuse_golden_update(None).expect("not set");
        let err = refuse_golden_update(Some("1".into())).expect_err("set");
        assert!(err.to_string().contains("UPDATE_GOLDEN"), "{err}");
    }

    #[test]
    fn unimplemented_and_unknown_jobs_are_refused_up_front() {
        let repo = clean_repo("ci-refused");
        let err = run(&args(&["perf"]), repo.path()).expect_err("not implemented");
        assert!(err.to_string().contains("not implemented"), "{err}");
        let err = run(&args(&["deploy"]), repo.path()).expect_err("unknown");
        assert!(err.to_string().contains("unknown job"), "{err}");
        assert!(!repo.path().join(".private").exists());
    }
}
