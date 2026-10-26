//! `cargo xtask compat` (design 5.9.5): the compat scenarios through the
//! host, with the fake agent (`--fake`) or the real CLI (`--real`). The
//! runner, an ignored test of cupper-host (`tests/compat.rs`), plays them and
//! writes what happened; this makes the report of it, compares the real
//! configuration before and after a real run ([`config`]), and with
//! `--write` takes the recordings into `testdata/` ([`record`]).
//! `cargo xtask compat-reports` checks the committed reports ([`reports`]).

mod config;
mod record;
pub mod report;
pub mod reports;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::json;

use crate::ci::git;
use crate::clock::Stopwatch;
use crate::runtime_pack;
use report::{Mode, Named, Outcome, Report};

pub const USAGE: &str = "usage: cargo xtask compat (--fake | --real [--model <model>]) [--dry-run]
         [--only <scenario>]... [--out <dir>] [--write [--words <file>]]

  --fake     the fake agent plays the CLI: no quota
  --real     the real CLI, with the user's own configuration: uses quota;
             the configuration is compared before and after
  --model    the model of a real run (haiku)
  --dry-run  the plan and the self-constraints' verdicts; nothing starts
  --only     a scenario to run; all by default
  --out      where the report and the recordings go (target/compat, emptied first)
  --write    take the recordings (the GOLD-B ones of a fake run; all of a real
             run, which must be whole, and its report) into testdata/ and update
             the goldens; needs the sensitive word list,
             .private/sensitive-words.txt of the main work tree or --words";

/// Where `cargo xtask host-bundle --kind e2e` puts the host a real run uses.
const HOST: &str = "target/host-bundle/e2e/CupperHost.app/Contents/MacOS/cupper-host";

#[derive(Debug, Default, PartialEq)]
struct Options {
    real: bool,
    model: Option<String>,
    dry_run: bool,
    only: Vec<String>,
    out: Option<PathBuf>,
    write: bool,
    words: Option<PathBuf>,
}

/// What the runner writes as `run.json`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Run {
    adapter: Named,
    cli: Named,
    /// The temporary roots of the scenarios' projects.
    roots: Vec<PathBuf>,
    /// The CLI's sessions the scenarios started.
    cli_sessions: Vec<String>,
    scenarios: Vec<report::Scenario>,
}

pub fn main(args: &[String]) -> ExitCode {
    let ran = parse(args).and_then(|options| {
        let root = git::toplevel(&std::env::current_dir()?)?;
        run(&root, &options)
    });
    match ran {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("xtask compat: {err:#}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn parse(args: &[String]) -> Result<Options> {
    let (mut options, mut fake) = (Options::default(), false);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = || {
            rest.next()
                .with_context(|| format!("`{arg}` needs a value"))
        };
        match arg.as_str() {
            "--fake" => fake = true,
            "--real" => options.real = true,
            "--model" => options.model = Some(value()?.clone()),
            "--dry-run" => options.dry_run = true,
            "--only" => options.only.push(value()?.clone()),
            "--out" => options.out = Some(value()?.into()),
            "--write" => options.write = true,
            "--words" => options.words = Some(value()?.into()),
            other => bail!("unexpected argument `{other}`"),
        }
    }
    // T188 counts a real report that passes everything toward the verified
    // range; one of some scenarios would pass everything too.
    let partial = options.real && options.write && !options.only.is_empty();
    let refused = [
        (
            fake == options.real,
            "one of `--fake` and `--real` is needed, not both",
        ),
        (
            !options.real && options.model.is_some(),
            "`--model` goes with `--real`",
        ),
        (
            !options.write && options.words.is_some(),
            "`--words` goes with `--write`",
        ),
        (options.write && options.dry_run, "a dry run writes nothing"),
        (
            partial,
            "a real run that `--write` keeps plays every scenario: no `--only`",
        ),
    ];
    match refused.iter().find(|(refused, _)| *refused) {
        Some((_, why)) => bail!("{why}"),
        None => Ok(options),
    }
}

/// Whether every scenario passed, or did what it can, and the real
/// configuration stayed as it was.
fn run(root: &Path, options: &Options) -> Result<bool> {
    let out = options
        .out
        .clone()
        .unwrap_or_else(|| root.join("target/compat"));
    let words = options
        .write
        .then(|| record::words(root, options.words.as_deref()));
    let words = words.transpose()?;
    if out.exists() {
        fs::remove_dir_all(&out).with_context(|| format!("empty {}", out.display()))?;
    }
    fs::create_dir_all(&out)?;
    let watch = Stopwatch::start();
    let cargo = || {
        let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
        cargo.current_dir(root);
        cargo
    };
    let xtask = |args: &str| -> Result<()> {
        let mut xtask = Command::new(std::env::current_exe()?);
        let status = xtask.current_dir(root).args(args.split(' ')).status()?;
        ensure!(status.success(), "`xtask {args}` failed");
        Ok(())
    };
    let model = options.model.clone().unwrap_or_else(|| "haiku".into());
    let real = json!({"host": root.join(HOST), "model": model});
    let real = options.real.then_some(real);
    // A dry run starts nothing, so it needs neither the host nor the fake agent.
    match (options.dry_run, options.real) {
        (true, _) => {}
        (false, true) => {
            if !runtime_pack::staged(root)? {
                xtask("runtime-pack")?;
            }
            xtask("host-bundle --kind e2e --debug")?;
        }
        (false, false) => {
            let built = cargo()
                .args(["build", "-p", "cupper-fake-agent", "--locked"])
                .status()?;
            ensure!(built.success(), "the fake agent did not build");
        }
    }
    let home = PathBuf::from(std::env::var_os("HOME").context("HOME")?);
    let before = (options.real && !options.dry_run).then(|| config::snapshot(&home));
    let before = before.transpose()?;
    let plan = out.join("plan.json");
    let attempts = if options.real { 3 } else { 1 };
    let plan_json = json!({"only": options.only, "attempts": attempts, "out": out,
        "dry_run": options.dry_run, "real": real});
    fs::write(&plan, plan_json.to_string())?;
    let mut runner = cargo();
    runner.args([
        "test",
        "-p",
        "cupper-host",
        "--features",
        "test-harness",
        "--locked",
    ]);
    runner.args([
        "--test",
        "compat",
        "--",
        "--ignored",
        "--exact",
        "compat_driver",
    ]);
    let status = runner
        .arg("--nocapture")
        .env("CUPPER_COMPAT_PLAN", &plan)
        .status()?;
    ensure!(status.success(), "the runner failed");
    if options.dry_run {
        let hashed = options.real.then(config::names);
        let hashed = hashed.unwrap_or_else(|| "none".into());
        println!("configuration compared before and after: {hashed}");
        return Ok(true);
    }

    let run: Run = serde_json::from_str(&fs::read_to_string(out.join("run.json"))?)
        .context("read the runner's run.json")?;
    let (config_files, mut cleanup) = match &before {
        Some(before) => config::compare(before, &config::snapshot(&home)?, &run.roots),
        None => (Vec::new(), Vec::new()),
    };
    cleanup.extend(
        run.cli_sessions
            .iter()
            .map(|id| format!("the CLI's session {id}")),
    );
    let report = Report {
        v: 1,
        commit: git::head(root)?,
        mode: if options.real {
            Mode::RealConfig
        } else {
            Mode::Fake
        },
        agent: "claude".into(),
        model: options.real.then_some(model),
        adapter: run.adapter,
        cli: run.cli,
        duration_ms: watch.elapsed_ms(),
        scenarios: run.scenarios,
        config_files,
        cleanup,
    };
    let text = serde_json::to_string_pretty(&report)? + "\n";
    let problems = report::problems(&report::parse(&text)?);
    ensure!(
        problems.is_empty(),
        "the report does not conform: {problems:?}"
    );
    let path = out.join("report.json");
    fs::write(&path, &text)?;
    for s in &report.scenarios {
        let detail = s.detail.as_deref().unwrap_or_default();
        println!("  {:<16} {:<15} {detail}", s.id, format!("{:?}", s.outcome));
    }
    for check in report.config_files.iter().filter(|check| check.changed) {
        println!("  CHANGED by the run: {}", check.name);
    }
    report
        .cleanup
        .iter()
        .for_each(|item| println!("  to clean up: {item}"));
    println!("report: {}", path.display());
    let failed = [Outcome::Fail, Outcome::Inconclusive];
    let passed = !report.scenarios.iter().any(|s| failed.contains(&s.outcome))
        && report.config_files.iter().all(|check| !check.changed);
    if let Some(words) = words {
        ensure!(passed, "testdata/ left as it is: the run did not pass");
        record::write(root, &out, &run.roots, &report, &words)?;
    }
    Ok(passed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &str) -> Vec<String> {
        line.split_whitespace().map(Into::into).collect()
    }

    #[test]
    fn unit_compat_guards_a_real_run_is_asked_for_and_kept_whole() {
        let options = parse(&args("--fake --only cancel --only plan --write --words w")).unwrap();
        assert_eq!(options.only, ["cancel", "plan"]);
        assert_eq!((options.write, options.words), (true, Some("w".into())));
        let real = parse(&args("--real --model sonnet --dry-run --only plan")).unwrap();
        assert_eq!(
            (real.real, real.model, real.dry_run),
            (true, Some("sonnet".into()), true)
        );
        assert!(parse(&args("--real --write")).is_ok());
        for bad in [
            "--only cancel",
            "--fake --real",
            "--fake --model haiku",
            "--fake --words w",
            "--fake --only",
            "--fake --dry-run --write",
            "--real --write --only plan",
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad}");
        }
    }
}
