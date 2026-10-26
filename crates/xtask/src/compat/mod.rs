//! `cargo xtask compat` (design 5.9.5): the compat scenarios through the host,
//! so far with the fake agent playing the CLI. The runner, an ignored test of
//! cupper-host (`tests/compat.rs`), plays them and writes what happened; this
//! makes the report of it, and with `--write` takes the GOLD-B scenarios'
//! recordings into `testdata/` ([`record`]).

mod record;
pub mod report;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use serde_json::json;

use crate::ci::git;
use crate::clock::Stopwatch;
use report::{Mode, Named, Outcome, Report};

pub const USAGE: &str = "usage: cargo xtask compat --fake [--only <scenario>]... [--out <dir>] [--write [--words <file>]]

  --fake    the fake agent plays the CLI: no quota (the real CLIs are not driven yet)
  --only    a scenario to run; all by default
  --out     where the report and the recordings go (target/compat, emptied first)
  --write   take the GOLD-B scenarios' recordings into testdata/ and update their
            goldens; needs the sensitive word list, .private/sensitive-words.txt
            of the main work tree or --words";

#[derive(Debug, Default, PartialEq)]
struct Options {
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
    /// The temporary roots of the scenarios' projects, for `tools/sanitize`.
    roots: Vec<PathBuf>,
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
            "--only" => options.only.push(value()?.clone()),
            "--out" => options.out = Some(value()?.into()),
            "--write" => options.write = true,
            "--words" => options.words = Some(value()?.into()),
            other => bail!("unexpected argument `{other}`"),
        }
    }
    ensure!(fake, "`--fake` is needed: the real CLIs are not driven yet");
    ensure!(
        options.write || options.words.is_none(),
        "`--words` goes with `--write`"
    );
    Ok(options)
}

/// Whether every scenario passed.
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
    let built = cargo()
        .args(["build", "-p", "cupper-fake-agent", "--locked"])
        .status()?;
    ensure!(built.success(), "the fake agent did not build");
    let plan = out.join("plan.json");
    fs::write(
        &plan,
        json!({"only": options.only, "attempts": 1, "out": out}).to_string(),
    )?;
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

    let run: Run = serde_json::from_str(&fs::read_to_string(out.join("run.json"))?)
        .context("read the runner's run.json")?;
    let report = Report {
        v: 1,
        commit: git::head(root)?,
        mode: Mode::Fake,
        agent: "claude".into(),
        model: None,
        adapter: run.adapter,
        cli: run.cli,
        duration_ms: watch.elapsed_ms(),
        scenarios: run.scenarios,
        config_files: Vec::new(),
        cli_sessions: Vec::new(),
    };
    let text = serde_json::to_string_pretty(&report)? + "\n";
    let problems = report::problems(&report::parse(&text)?);
    ensure!(
        problems.is_empty(),
        "the report does not conform: {problems:?}"
    );
    let path = out.join("report.json");
    fs::write(&path, text)?;
    for s in &report.scenarios {
        let detail = s.detail.as_deref().unwrap_or_default();
        println!("  {:<16} {:<13} {detail}", s.id, format!("{:?}", s.outcome));
    }
    println!("report: {}", path.display());
    let passed = report.scenarios.iter().all(|s| s.outcome == Outcome::Pass);
    if let Some(words) = words {
        ensure!(passed, "testdata/ left as it is: not every scenario passed");
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
    fn compat_runs_only_the_fake_agent_so_far() {
        let options = parse(&args("--fake --only cancel --only plan --write --words w")).unwrap();
        assert_eq!(options.only, ["cancel", "plan"]);
        assert_eq!((options.write, options.words), (true, Some("w".into())));
        assert!(parse(&args("--only cancel")).is_err());
        assert!(parse(&args("--fake --words w")).is_err());
        assert!(parse(&args("--fake --only")).is_err());
    }
}
