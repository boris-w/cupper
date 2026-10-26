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
        let shown = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        match report::parse(&fs::read_to_string(&path)?) {
            Ok(read) => {
                let found = report::problems(&read);
                problems.extend(
                    found
                        .into_iter()
                        .map(|problem| format!("{shown}: {problem}")),
                );
                reports.push((shown, read));
            }
            Err(err) => problems.push(format!("{shown}: {err}")),
        }
    }
    let path = root.join(MANIFEST);
    let mut manifest = Manifest::load(&path)?;
    let pinned = [
        manifest.claude_agent_acp.version.as_str(),
        manifest.codex_acp.version.as_str(),
    ];
    let (spans, found) = verified(&reports, pinned);
    problems.extend(found);
    let ranges = [&mut manifest.cli.claude, &mut manifest.cli.codex];
    for ((range, span), (cli, ..)) in ranges.into_iter().zip(spans).zip(BASELINE) {
        if write {
            range.verified = span;
        } else if range.verified != span {
            let shown = |span: &VersionSpan| format!("{}-{}", span.min, span.max);
            problems.push(format!(
                "{MANIFEST}: cli.{cli}.verified is {}, the reports give {}: \
                 cargo xtask compat-reports --write",
                shown(&range.verified),
                shown(&span)
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
        let Some(at) = BASELINE.iter().position(|(name, ..)| *name == cli.name) else {
            problems.push(format!(
                "{shown}: the CLI `{}` is neither claude nor codex",
                cli.name
            ));
            continue;
        };
        let Some(found) = version(&cli.version) else {
            problems.push(format!(
                "{shown}: the CLI version `{}` is not x.y.z",
                cli.version
            ));
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
/// adapter, `adapter`, in which every scenario passed. One that failed or
/// was inconclusive keeps the whole report out: the structure was not
/// checked on that version, and it counts once a full run passes.
fn counts(report: &Report, adapter: &str) -> bool {
    let passed = |outcome| match outcome {
        Outcome::Pass => true,
        Outcome::Fail | Outcome::Inconclusive => false,
    };
    report.mode != Mode::Fake
        && report.adapter.version == adapter
        && !report.scenarios.is_empty()
        && report.scenarios.iter().all(|s| passed(s.outcome))
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
