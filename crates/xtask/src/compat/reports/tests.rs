use serde_json::{Value, json};

use super::*;
use crate::testutil::TempDir;

/// The pinned claude-agent-acp and codex-acp of the committed manifest.
const PINNED: [&str; 2] = ["0.91.0", "2.2.2"];

/// A run of `cli` at `version` on the adapter at `adapter`, a scenario per
/// outcome; a real one hashes a configuration file.
fn report(mode: &str, cli: &str, version: &str, adapter: &str, outcomes: &[&str]) -> Value {
    let scenario = |(i, outcome): (usize, &&str)| {
        json!({"id": format!("s{i}"), "title": "s",
        "outcome": outcome, "detail": (*outcome != "pass").then_some("not met: x"),
        "attempts": 1, "duration_ms": 100, "checks": [{"check": "x", "passed": *outcome == "pass"}],
        "gold_b": false, "recording": null})
    };
    let (real, hash) = (mode != "fake", "ab".repeat(32));
    let config = json!({"path": "~/.claude/settings.json", "before": hash, "after": hash});
    json!({"v": 1, "commit": "0123456789abcdef0123456789abcdef01234567", "mode": mode,
        "agent": cli, "model": real.then_some("haiku"), "duration_ms": 1000,
        "adapter": {"name": "adapter", "version": adapter}, "cli": {"name": cli, "version": version},
        "scenarios": outcomes.iter().enumerate().map(scenario).collect::<Vec<_>>(),
        "config_files": if real { vec![config] } else { vec![] }, "cli_sessions": []})
}

fn real(cli: &str, version: &str, adapter: &str, outcomes: &[&str]) -> Value {
    report("real_config", cli, version, adapter, outcomes)
}

/// The range [`verified`] gives each CLI, as `min-max`, and its problems.
fn ranges(reports: &[Value]) -> (Vec<String>, Vec<String>) {
    let read = |r: &Value| ("r.json".into(), report::parse(&r.to_string()).unwrap());
    let (spans, problems) = verified(&reports.iter().map(read).collect::<Vec<_>>(), PINNED);
    let spans = spans.iter().map(|s| format!("{}-{}", s.min, s.max));
    (spans.collect(), problems)
}

fn put(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn unit_compat_tools_range_is_the_baseline_widened_by_counted_reports() {
    let baseline = ["2.1.200-2.1.296", "0.150.1-0.163.0"].map(String::from);
    assert_eq!(ranges(&[]), (baseline.to_vec(), vec![]));
    let [claude, codex] = PINNED;
    let reports = [
        real("claude", "2.1.1000", claude, &["pass", "pass"]),
        report("isolated_config", "codex", "0.149.0", codex, &["pass"]),
        // Left out: a fake run, another adapter, a failed, an inconclusive
        // and an empty run.
        report("fake", "claude", "fake", claude, &["pass"]),
        real("claude", "2.1.2000", "0.90.0", &["pass"]),
        real("claude", "2.1.3000", claude, &["pass", "fail"]),
        real("claude", "2.1.4000", claude, &["inconclusive"]),
        real("codex", "0.1.0", codex, &[]),
    ];
    let wider = ["2.1.200-2.1.1000", "0.149.0-0.163.0"].map(String::from);
    assert_eq!(ranges(&reports), (wider.to_vec(), vec![]));

    let wrong = [
        real("claude", "2.1", claude, &["pass"]),
        real("gemini", "1.0.0", claude, &["pass"]),
    ];
    let problems = [
        "r.json: the CLI `claude 2.1` is not claude or codex at x.y.z",
        "r.json: the CLI `gemini 1.0.0` is not claude or codex at x.y.z",
    ]
    .map(String::from);
    assert_eq!(ranges(&wrong), (baseline.to_vec(), problems.to_vec()));
    for text in ["+1.0.0", "1.0.0.0", "1..0", "1.0.x", ""] {
        assert_eq!(version(text), None, "{text}");
    }
}

#[test]
fn unit_compat_tools_reports_conform_and_give_the_manifest_its_ranges() {
    let dir = TempDir::new("compat-reports");
    let root = dir.path();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    put(root, report::SCHEMA, &report::schema());
    let manifest = fs::read_to_string(repo.join(MANIFEST)).unwrap();
    put(root, MANIFEST, &manifest);
    let before = Manifest::load(&root.join(MANIFEST)).unwrap();
    let check = |write| run(root, write).unwrap();
    assert_eq!(check(false), Vec::<String>::new(), "no reports");

    let good = real("claude", "2.1.1000", PINNED[0], &["pass"]);
    put(root, &format!("{DIR}/claude/good.json"), &good.to_string());
    let mut stray = good.clone();
    stray["scenarios"][0]["stray"] = true.into();
    put(root, &format!("{DIR}/stray.json"), &stray.to_string());
    let mut head = good;
    head["commit"] = "HEAD".into();
    put(root, &format!("{DIR}/head.json"), &head.to_string());
    put(root, &format!("{DIR}/notes.txt"), "not a report");
    // Nothing is written while a report is wrong, though `good` widens the range.
    let problems = check(true);
    assert_eq!(problems.len(), 2, "{problems:#?}");
    let head = format!("{DIR}/head.json: the commit is not a full hash");
    assert_eq!(problems[0], head);
    assert!(problems[1].starts_with(&format!("{DIR}/stray.json: unknown field `stray`")));
    assert_eq!(Manifest::load(&root.join(MANIFEST)).unwrap(), before);

    fs::remove_file(root.join(DIR).join("stray.json")).unwrap();
    fs::remove_file(root.join(DIR).join("head.json")).unwrap();
    let stale = "runtime/manifest.json: cli.claude.verified is 2.1.200-2.1.296, the reports \
        give 2.1.200-2.1.1000: cargo xtask compat-reports --write";
    assert_eq!(check(false), [stale]);
    assert_eq!(check(true), Vec::<String>::new());
    let mut want = before;
    want.cli.claude.verified.max = "2.1.1000".into();
    assert_eq!(Manifest::load(&root.join(MANIFEST)).unwrap(), want);
    assert_eq!(check(false), Vec::<String>::new());

    fs::write(root.join(report::SCHEMA), "{}\n").unwrap();
    let schema = format!(
        "{} is stale: UPDATE_GOLDEN=1 cargo test -p xtask",
        report::SCHEMA
    );
    assert_eq!(check(false), [schema]);
}
