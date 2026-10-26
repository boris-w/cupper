//! `cargo xtask compat-reports [--write]` (design 5.9.5, 5.10.3): the compat
//! reports committed under [`DIR`] conform, the report schema is current,
//! and the verified CLI ranges of `runtime/manifest.json` are the ones the
//! reports give; `testdata-scan` runs it. Nobody writes the ranges by hand:
//! `--write` does. A range starts from ADR-08's ([`BASELINE`]) and widens to
//! the CLI version of every report that counts ([`counts`]).

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;

use super::report::{self, Mode, Outcome, Report};
use crate::ci::git;
use crate::runtime_pack::manifest::{Manifest, VersionSpan};

pub const USAGE: &str = "usage: cargo xtask compat-reports [--write]

  checks that every report under testdata/compat/reports/ conforms, that
  testdata/compat/report.schema.json is current, and that the verified CLI
  ranges in runtime/manifest.json are the ones the reports give
  --write   rewrite those ranges from the reports";

/// The committed reports: every `.json` file in it, subdirectories included.
pub const DIR: &str = "testdata/compat/reports";
const MANIFEST: &str = "runtime/manifest.json";
const WRITE: &str = "cargo xtask compat-reports --write";

/// ADR-08's verified ranges, before any report: the CLI, its lowest and its
/// highest version. The order of `cli` in the manifest.
const BASELINE: [(&str, &str, &str); 2] = [
    ("claude", "2.1.200", "2.1.296"),
    ("codex", "0.150.1", "0.163.0"),
];

pub fn main(args: &[String]) -> ExitCode {
    let write = match args {
        [] => false,
        [flag] if flag == "--write" => true,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let ran = std::env::current_dir()
        .map_err(Into::into)
        .and_then(|cwd| git::toplevel(&cwd))
        .and_then(|root| run(&root, write));
    match ran {
        Ok(problems) if problems.is_empty() => {
            println!("the compat reports conform and give the verified CLI ranges of {MANIFEST}");
            ExitCode::SUCCESS
        }
        Ok(problems) => {
            problems.iter().for_each(|problem| eprintln!("{problem}"));
            ExitCode::FAILURE
        }
        Err(err) => {
            eprintln!("xtask compat-reports: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// What is wrong in the work tree at `root`; with `write`, the ranges are
/// written instead of compared, unless something else is wrong.
fn run(root: &Path, write: bool) -> Result<Vec<String>> {
    let mut problems = Vec::new();
    if fs::read_to_string(root.join(report::SCHEMA)).ok() != Some(report::schema()) {
        let stale = "is stale: UPDATE_GOLDEN=1 cargo test -p xtask";
        problems.push(format!("{} {stale}", report::SCHEMA));
    }
    let mut reports = Vec::new();
    for path in files(&root.join(DIR))? {
        let shown = path.strip_prefix(root)?.display().to_string();
        let read = match report::parse(&fs::read_to_string(&path)?) {
            Ok(read) => read,
            Err(err) => {
                problems.push(format!("{shown}: {err}"));
                continue;
            }
        };
        problems.extend(
            report::problems(&read)
                .iter()
                .map(|p| format!("{shown}: {p}")),
        );
        reports.push((shown, read));
    }
    let path = root.join(MANIFEST);
    let mut manifest = Manifest::load(&path)?;
    let pinned = [
        &manifest.claude_agent_acp.version,
        &manifest.codex_acp.version,
    ];
    let (spans, found) = verified(&reports, pinned.map(String::as_str));
    problems.extend(found);
    let ranges = [&mut manifest.cli.claude, &mut manifest.cli.codex];
    for ((range, span), (cli, ..)) in ranges.into_iter().zip(spans).zip(BASELINE) {
        if write {
            range.verified = span;
        } else if range.verified != span {
            let shown = |span: &VersionSpan| format!("{}-{}", span.min, span.max);
            let (now, given) = (shown(&range.verified), shown(&span));
            problems.push(format!(
                "{MANIFEST}: cli.{cli}.verified is {now}, the reports give {given}: {WRITE}"
            ));
        }
    }
    if write && problems.is_empty() {
        fs::write(&path, manifest.to_text())?;
        println!("wrote the verified CLI ranges of {MANIFEST}");
    }
    Ok(problems)
}

/// The `.json` files under `dir`, sorted; none when it does not exist.
fn files(dir: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(dir) {
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        entries => entries?,
    };
    let mut found = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.is_dir() {
            found.extend(files(&path)?);
        } else if path.extension().is_some_and(|ext| ext == "json") {
            found.push(path);
        }
    }
    found.sort();
    Ok(found)
}

/// The verified range of each CLI of [`BASELINE`], given the reports (each
/// with the path it is shown by) and the pinned adapters' versions; and what
/// is wrong with a real run's CLI.
fn verified(reports: &[(String, Report)], pinned: [&str; 2]) -> (Vec<VersionSpan>, Vec<String>) {
    let mut problems = Vec::new();
    let mut spans = BASELINE.map(|(_, min, max)| [min, max].map(|v| version(v).expect("x.y.z")));
    for (shown, report) in reports.iter().filter(|(_, r)| r.mode != Mode::Fake) {
        let cli = &report.cli;
        let at = BASELINE.iter().position(|(name, ..)| *name == cli.name);
        let (Some(at), Some(found)) = (at, version(&cli.version)) else {
            let what = format!(
                "`{} {}` is not claude or codex at x.y.z",
                cli.name, cli.version
            );
            problems.push(format!("{shown}: the CLI {what}"));
            continue;
        };
        if counts(report, pinned[at]) {
            let [min, max] = &mut spans[at];
            (*min, *max) = ((*min).min(found), (*max).max(found));
        }
    }
    let shown = |[a, b, c]: [u64; 3]| format!("{a}.{b}.{c}");
    let spans = spans.map(|[min, max]| VersionSpan {
        min: shown(min),
        max: shown(max),
    });
    (spans.into(), problems)
}

/// Whether a report widens its CLI's range: a real run of the pinned
/// adapter, `adapter`, in which a scenario passed and none failed. One that
/// failed or was inconclusive keeps the whole report out: the structure was
/// not checked on that version, and it counts once a full run passes. A
/// scenario that does not apply, or that only records what it saw, does not
/// keep it out; `real_only` never comes in a real run.
fn counts(report: &Report, adapter: &str) -> bool {
    let fine = |outcome| match outcome {
        Outcome::Pass | Outcome::NotApplicable | Outcome::Observed | Outcome::RealOnly => true,
        Outcome::Fail | Outcome::Inconclusive => false,
    };
    report.mode != Mode::Fake
        && report.adapter.version == adapter
        && report.scenarios.iter().any(|s| s.outcome == Outcome::Pass)
        && report.scenarios.iter().all(|s| fine(s.outcome))
}

/// `x.y.z`, each a decimal number.
fn version(text: &str) -> Option<[u64; 3]> {
    let number = |part: &str| {
        let digits = !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());
        digits.then(|| part.parse().ok()).flatten()
    };
    let parts: Vec<u64> = text.split('.').map(number).collect::<Option<_>>()?;
    parts.try_into().ok()
}

#[cfg(test)]
mod tests;
