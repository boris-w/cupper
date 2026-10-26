//! `cargo xtask size`: the lines a task branch has
//! changed, against the stop lines of its brief (rule 8 of the sub-agent
//! guide). Counted as task reports count: tests included; lock files,
//! generated files, `docs/`, test data and screenshot baselines left out.
//! Main merged into the branch is in the base, so it never counts.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::ci::git;

pub const USAGE: &str = "usage: cargo xtask size [--task <ID>] [--json] [--rev <commit> --main <commit>]\n\n\
    Lines the branch changed since main, the work tree included (new files once added), against\n\
    the stop lines in docs/tasks/<ID>-*/brief.md; the ID defaults to the branch's.\n\
    --rev, --main: a past state instead, e.g. a task's last commit and the main it merged into.";

/// Paths left out: a prefix ending in `/`, else the whole path or its file name.
pub const EXCLUDED: &[&str] = &[
    "Cargo.lock",
    "pnpm-lock.yaml",
    "docs/",
    "testdata/",
    "tools/ui-screenshots/baselines/",
    "packages/protocol/src/gen/",
    "protocol/schema/",
    "protocol/fixtures/views.json",
    "runtime/manifest.json",
    "runtime/locks/",
    "runtime/licenses/",
];

pub fn excluded(path: &str) -> bool {
    EXCLUDED.iter().any(|ex| match ex.strip_suffix('/') {
        Some(_) => path.starts_with(ex),
        None => path == *ex || path.rsplit('/').next() == Some(*ex),
    })
}

/// `crates/x`, `apps/x`, `packages/x`, `tools/x`; else the first component.
fn group(path: &str) -> String {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [
            top @ ("crates" | "apps" | "packages" | "tools"),
            name,
            _,
            ..,
        ] => format!("{top}/{name}"),
        [first, _, ..] => (*first).to_owned(),
        _ => path.to_owned(),
    }
}

/// `(stop line, hard stop)` from the template's line `- 停止线：<n>；硬停止线：<n>`,
/// the last one; without it, from the prose of the brief's sections on its
/// size and rulings, later ones overriding: "上限约 1,000", "超过约 850 行";
/// "硬停止线 950", "硬线 1,100".
pub fn stop_lines(brief: &str) -> (Option<u64>, Option<u64>) {
    let fixed = brief.lines().rfind(|line| line.starts_with("- 停止线："));
    let (mut stop, mut hard) = (None, None);
    let mut counted = fixed.is_some();
    for line in fixed.map_or_else(|| brief.lines().collect(), |line| vec![line]) {
        if let Some(title) = line.strip_prefix("## ") {
            counted = title.contains("规模") || title.contains("裁定");
            continue;
        }
        let mut rest = line;
        while counted && !rest.is_empty() {
            let found = ["硬停止线", "硬线", "停止线", "上限", "超过"]
                .iter()
                .filter_map(|key| rest.find(key).map(|at| (at, *key)))
                .min();
            let Some((at, key)) = found else { break };
            rest = &rest[at + key.len()..];
            let number = rest.trim_start_matches(['：', ':', ' ', '约']);
            let digits: String = number
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == ',')
                .collect();
            if let Ok(n) = digits.replace(',', "").parse() {
                *if key.starts_with('硬') {
                    &mut hard
                } else {
                    &mut stop
                } = Some(n);
            }
        }
    }
    (stop, hard)
}

/// The base: the newest main the branch contains, by the local `main` or a
/// main merged in later than it.
fn base(root: &Path, rev: &str, main: &str) -> Result<String> {
    let mut mains = vec![main.to_owned()];
    let merges = git::git(
        root,
        &["rev-list", "--first-parent", "--merges", rev, "--not", main],
    )?;
    mains.extend(merges.lines().map(|merge| format!("{merge}^2")));
    let mut args = vec!["merge-base", "--independent"];
    args.extend(mains.iter().map(String::as_str));
    let newest = git::git(root, &args)?;
    let newest = newest.lines().next().context("no main to count from")?;
    git::git(root, &["merge-base", rev, newest])
}

pub fn main(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask size: {err:#}\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run(args: &[String]) -> Result<()> {
    let (mut task, mut as_json, mut rev, mut main) = (None, false, None, "main".to_owned());
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = |name| {
            rest.next()
                .cloned()
                .with_context(|| format!("`{name}` needs a value"))
        };
        match arg.as_str() {
            "--task" => task = Some(value("--task")?),
            "--rev" => rev = Some(value("--rev")?),
            "--main" => main = value("--main")?,
            "--json" => as_json = true,
            other => bail!("unexpected argument `{other}`"),
        }
    }
    let root = git::toplevel(&std::env::current_dir()?)?;
    let branch = git::git(&root, &["branch", "--show-current"])?;
    let task = task.or_else(|| {
        let name = branch.rsplit('/').next()?;
        Some(name.split('-').next()?.to_owned()).filter(|id| id.starts_with('T'))
    });
    let base = base(&root, rev.as_deref().unwrap_or("HEAD"), &main)?;
    // Against the work tree, unless a past commit is asked for.
    let mut diff = vec!["diff", "--numstat", &base];
    diff.extend(rev.as_deref());
    let mut dirs: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    for line in git::git(&root, &diff)?.lines() {
        let mut fields = line.splitn(3, '\t');
        let (Some(add), Some(del), Some(path)) = (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        // Binary files show `-`.
        if let (Ok(add), Ok(del), false) = (add.parse::<u64>(), del.parse::<u64>(), excluded(path))
        {
            let entry = dirs.entry(group(path)).or_default();
            *entry = (entry.0 + add, entry.1 + del);
        }
    }
    let (added, deleted) = dirs.values().fold((0, 0), |(a, d), (x, y)| (a + x, d + y));
    let brief = task.as_ref().and_then(|id| {
        let dirs = fs::read_dir(root.join("docs/tasks")).ok()?;
        let dir = dirs.flatten().find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with(&format!("{id}-"))
        })?;
        fs::read_to_string(dir.path().join("brief.md")).ok()
    });
    let (stop, hard) = brief.as_deref().map_or((None, None), stop_lines);
    let status = match (stop, hard) {
        (_, Some(h)) if added > h => "over_hard_stop",
        (Some(s), _) if added > s => "over_stop_line",
        (Some(s), _) if added * 10 >= s * 9 => "near_stop_line",
        (Some(_), _) => "ok",
        _ => "no_stop_line",
    };
    if as_json {
        let by_dir: Vec<_> = dirs
            .iter()
            .map(|(dir, (a, d))| json!({"dir": dir, "added": a, "deleted": d}))
            .collect();
        let report = json!({"task": task, "base": base, "added": added, "deleted": deleted,
            "net": added as i64 - deleted as i64, "stop_line": stop, "hard_stop": hard,
            "status": status, "by_dir": by_dir, "excluded": EXCLUDED});
        println!("{report}");
        return Ok(());
    }
    let id = task.as_deref().unwrap_or("this branch");
    let net = added as i64 - deleted as i64;
    let against = match (stop, status) {
        (None, _) => "no stop line found in the brief".to_owned(),
        (Some(s), "ok") => format!("{} left to the stop line {s}", s - added),
        (Some(s), "near_stop_line") => format!("!! {} left to the stop line {s}", s - added),
        (Some(s), _) => format!("!! {} over the stop line {s}", added - s),
    };
    let hard = hard.map_or(String::new(), |h| format!(", hard stop {h}"));
    println!("{id}: +{added} -{deleted} (net {net:+}); {against}{hard}");
    for (dir, (a, d)) in &dirs {
        println!("  {dir:<28} +{a} -{d}");
    }
    println!("left out: {}", EXCLUDED.join(", "));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{clean_repo, git};

    #[test]
    fn size_leaves_out_lock_generated_docs_data_and_baselines() {
        for path in [
            "Cargo.lock",
            "crates/x/Cargo.lock",
            "pnpm-lock.yaml",
            "docs/tasks/T1/notes.md",
            "testdata/golden/a.jsonl",
            "tools/ui-screenshots/baselines/shell.png",
            "packages/protocol/src/gen/zod.gen.ts",
            "runtime/manifest.json",
        ] {
            assert!(excluded(path), "{path}");
        }
        for path in [
            "crates/xtask/src/size.rs",
            "tools/e2e/src/fixture.ts",
            "tests/ids.toml",
        ] {
            assert!(!excluded(path), "{path}");
        }
        assert_eq!(group("crates/xtask/src/size.rs"), "crates/xtask");
        assert_eq!(group("tests/ids.toml"), "tests");
        assert_eq!(group("package.json"), "package.json");
    }

    #[test]
    fn size_stop_lines_from_the_size_section_and_later_rulings() {
        let brief = "## 注意\n- 单处修复不超过 100 行\n\n## 规模\n推算超过约 850 行就停下；硬停止线 950。\n";
        assert_eq!(stop_lines(brief), (Some(850), Some(950)));
        let ruled = format!("{brief}\n## 补充裁定\n- 上限约 1,000 行，硬停止线 1,100。\n");
        assert_eq!(stop_lines(&ruled), (Some(1000), Some(1100)));
        // The template's line wins over any prose, later rulings included.
        let fixed = "## 规模\n- 停止线：600；硬停止线：680\n\n## 补充裁定\n上限约 700\n";
        assert_eq!(stop_lines(fixed), (Some(600), Some(680)));
        assert_eq!(stop_lines("## 规模\nS，估约 150 行。\n"), (None, None));
        assert_eq!(stop_lines("## 注意\n不超过 100 行\n"), (None, None));
    }

    #[test]
    fn size_main_merged_into_the_branch_is_in_the_base() {
        let repo = clean_repo("size-merge");
        let dir = repo.path();
        let write = |name: &str, lines: usize| {
            fs::write(dir.join(name), "x\n".repeat(lines)).expect("write")
        };
        let commit = |message: &str| {
            git(dir, &["add", "-A"]);
            git(dir, &["commit", "--quiet", "-m", message]);
        };
        git(dir, &["branch", "-M", "main"]);
        git(dir, &["checkout", "--quiet", "-b", "feat/T9-x"]);
        write("task.rs", 3);
        commit("task");
        git(dir, &["checkout", "--quiet", "main"]);
        write("other.rs", 50);
        commit("main moves");
        git(dir, &["checkout", "--quiet", "feat/T9-x"]);
        git(
            dir,
            &["merge", "--quiet", "--no-ff", "-m", "merge main", "main"],
        );
        // A local `main` older than the one merged must not bring it back.
        git(dir, &["branch", "-f", "main", "main~1"]);
        let base = base(dir, "HEAD", "main").expect("base");
        let diff = git::git(dir, &["diff", "--numstat", &base, "HEAD"]).expect("diff");
        assert_eq!(diff.trim(), "3\t0\ttask.rs");
    }
}
