//! `--write`: the recordings of the scenarios that passed into `testdata/`
//! (design 5.9.3), those of the GOLD-B scenarios for a fake run and all for
//! a real one, once `tools/sanitize` has redacted them with the run's
//! temporary roots and the local sensitive word list and found nothing left;
//! a real run's report, failures and all, into [`REPORTS`]; then the goldens
//! (`UPDATE_GOLDEN=1`) and the generic scan of `testdata/`. A recording that
//! differs from the committed one only in its times and request IDs stays as
//! it is, so a run that changed nothing changes no file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, ensure};
use serde_json::Value;

use super::report::{Mode, Outcome, Report, Scenario};
use crate::ci::jobs::testdata_scan::{WORD_LIST, word_list};

/// Where real runs' reports go, one per adapter and CLI version.
const REPORTS: &str = "testdata/compat/reports";

/// `given`, or the main work tree's list; an error when there is none.
pub fn words(root: &Path, given: Option<&Path>) -> Result<PathBuf> {
    let found = match given {
        Some(path) => path.is_file().then(|| path.to_owned()),
        None => word_list(root)?,
    };
    found.with_context(|| {
        format!("no sensitive word list ({WORD_LIST} of the main work tree, or `--words`): recordings enter testdata/ only past it")
    })
}

pub fn write(
    root: &Path,
    out: &Path,
    roots: &[PathBuf],
    report: &Report,
    words: &Path,
) -> Result<()> {
    let (staged, clean) = (out.join("staged"), out.join("sanitized"));
    fs::create_dir_all(&staged)?;
    let fake = report.mode == Mode::Fake;
    // What a scenario that failed recorded is no sample: its report says why.
    let took = |s: &Scenario| {
        let done = matches!(s.outcome, Outcome::Pass | Outcome::Observed);
        s.recording.is_some() && done && (s.gold_b || !fake)
    };
    let taken: Vec<_> = report.scenarios.iter().filter(|s| took(s)).collect();
    for scenario in &taken {
        let recording = scenario.recording.as_deref().context("a recording")?;
        fs::copy(
            out.join(recording),
            staged.join(format!("{}.jsonl", scenario.id)),
        )?;
    }
    let mut sanitize = node(root, &["sanitize", "--in"]);
    sanitize
        .arg(&staged)
        .arg("--out")
        .arg(&clean)
        .arg("--terms")
        .arg(words);
    for base in roots {
        sanitize.arg("--base").arg(base);
    }
    run(
        &mut sanitize,
        "tools/sanitize: something is left after redaction",
    )?;

    let (adapter, cli) = (&report.adapter, &report.cli);
    let dir = format!(
        "testdata/recordings/{}/{}/{}",
        adapter.name, adapter.version, cli.version
    );
    fs::create_dir_all(root.join(&dir))?;
    for scenario in &taken {
        let name = format!("{}.jsonl", scenario.id);
        let new = fs::read_to_string(clean.join(&name))?;
        let target = root.join(&dir).join(&name);
        let old = fs::read_to_string(&target).unwrap_or_default();
        if normalized(&old) == normalized(&new) {
            println!("  {dir}/{name}: unchanged");
        } else {
            fs::write(&target, new)?;
            println!("  {dir}/{name}: written");
        }
    }
    if !fake {
        // Its recordings named where they now are; it passes the word list too.
        let mut kept = report.clone();
        for s in &mut kept.scenarios {
            s.recording = took(s).then(|| format!("{dir}/{}.jsonl", s.id));
        }
        let name = format!(
            "{}-{}-{}-{}.json",
            adapter.name, adapter.version, cli.name, cli.version
        );
        let path = root.join(REPORTS).join(&name);
        fs::create_dir_all(root.join(REPORTS))?;
        fs::write(&path, serde_json::to_string_pretty(&kept)? + "\n")?;
        // `words` takes the paths it scans as relative to the work tree.
        let mut scan = node(root, &["words", "--list"]);
        let relative = format!("{REPORTS}/{name}");
        run(
            scan.arg(words).arg(relative),
            "the report holds a sensitive word",
        )?;
        println!("  {REPORTS}/{name}: written");
    }
    let mut goldens = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    goldens.current_dir(root).env("UPDATE_GOLDEN", "1");
    goldens.args([
        "test",
        "-p",
        "cupper-host",
        "--features",
        "test-harness",
        "--locked",
    ]);
    run(
        goldens.args(["--test", "gold_b"]),
        "the GOLD-B goldens were not written",
    )?;
    run(
        &mut node(root, &["scan", "testdata"]),
        "tools/sanitize found personal data",
    )
}

fn node(root: &Path, args: &[&str]) -> Command {
    let mut node = Command::new("node");
    node.current_dir(root)
        .arg("tools/sanitize/src/main.ts")
        .args(args);
    node
}

fn run(cmd: &mut Command, failed: &str) -> Result<()> {
    let status = cmd.status().with_context(|| format!("run {cmd:?}"))?;
    ensure!(status.success(), "{failed}");
    Ok(())
}

/// The frames without their times, the JSON-RPC IDs numbered in the order
/// they first appear; `None` for what is not JSON Lines.
fn normalized(text: &str) -> Option<Vec<Value>> {
    let mut ids = Vec::new();
    let frames = text.lines().map(|line| {
        let mut frame: Value = serde_json::from_str(line).ok()?;
        frame.as_object_mut()?.remove("t");
        if let Some(id) = frame.pointer_mut("/msg/id") {
            let seen = ids.iter().position(|seen| seen == id);
            *id = seen
                .unwrap_or_else(|| {
                    ids.push(id.clone());
                    ids.len() - 1
                })
                .into();
        }
        Some(frame)
    });
    frames.collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn compat_recordings_differing_in_times_and_ids_alone_are_the_same() {
        let header = r#"{"format":"cupper-rec","v":1}"#;
        let frames = |t: [u32; 3], ids: [&str; 2]| {
            let [a, b] = ids;
            format!(
                "{header}\n{{\"seq\":1,\"t\":{},\"msg\":{{\"id\":\"{a}\",\"method\":\"m\"}}}}\n\
                 {{\"seq\":2,\"t\":{},\"msg\":{{\"id\":\"{b}\"}}}}\n\
                 {{\"seq\":3,\"t\":{},\"msg\":{{\"id\":\"{a}\",\"result\":{{}}}}}}\n",
                t[0], t[1], t[2]
            )
        };
        let one = frames([0, 1, 2], ["x", "y"]);
        assert_eq!(
            normalized(&one),
            normalized(&frames([5, 9, 30], ["p", "q"]))
        );
        assert_ne!(normalized(&one), normalized(&frames([0, 1, 2], ["x", "x"])));
        assert_ne!(normalized(&one), normalized(&one.replace("\"m\"", "\"n\"")));
    }

    #[test]
    fn compat_write_needs_a_word_list() {
        let dir = TempDir::new("compat-words");
        let missing = dir.path().join("words.txt");
        let err = words(dir.path(), Some(&missing)).unwrap_err().to_string();
        assert!(err.contains(WORD_LIST), "{err}");
        fs::write(&missing, "# none\n").unwrap();
        assert_eq!(words(dir.path(), Some(&missing)).unwrap(), missing);
    }
}
