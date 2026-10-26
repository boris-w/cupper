use serde_json::json;

use super::*;
use crate::compat::report::{Check, ConfigFile, Named, Scenario};
use crate::testutil::TempDir;

const CLAUDE_ACP: &str = "0.91.0";
const CODEX_ACP: &str = "2.2.2";

fn report(mode: Mode, cli: &str, version: &str, adapter: &str, outcomes: &[Outcome]) -> Report {
    let scenario = |(index, &outcome): (usize, &Outcome)| Scenario {
        id: format!("s{index}"),
        title: "a scenario".into(),
        outcome,
        detail: (outcome != Outcome::Pass).then(|| "not met: x".into()),
        attempts: 1,
        duration_ms: 100,
        checks: vec![Check {
            check: "x".into(),
            passed: outcome == Outcome::Pass,
        }],
        gold_b: false,
        recording: None,
    };
    let hash = Some("ab".repeat(32));
    let config = ConfigFile {
        path: "~/.claude/settings.json".into(),
        before: hash.clone(),
        after: hash,
    };
    let named = |name: &str, version: &str| Named {
        name: name.into(),
        version: version.into(),
    };
    Report {
        v: 1,
        commit: "0123456789abcdef0123456789abcdef01234567".into(),
        mode,
        agent: cli.into(),
        model: (mode != Mode::Fake).then(|| "haiku".into()),
        adapter: named("adapter", adapter),
        cli: named(cli, version),
        duration_ms: 1000,
        scenarios: outcomes.iter().enumerate().map(scenario).collect(),
        config_files: if mode == Mode::Fake {
            Vec::new()
        } else {
            vec![config]
        },
        cli_sessions: Vec::new(),
    }
}

fn real(cli: &str, version: &str, adapter: &str) -> Report {
    report(Mode::RealConfig, cli, version, adapter, &[Outcome::Pass; 2])
}

fn ranges(reports: Vec<Report>) -> (Vec<String>, Vec<String>) {
    let reports: Vec<_> = reports.into_iter().map(|r| ("r.json".into(), r)).collect();
    let (spans, problems) = verified(&reports, [CLAUDE_ACP, CODEX_ACP]);
    let spans = spans
        .iter()
        .map(|s| format!("{}-{}", s.min, s.max))
        .collect();
    (spans, problems)
}

/// A work tree with the current schema, the committed manifest and no reports.
fn work_tree(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    let root = dir.path();
    fs::create_dir_all(root.join("testdata/compat")).unwrap();
    fs::write(root.join(report::SCHEMA), report::schema()).unwrap();
    fs::create_dir_all(root.join("runtime")).unwrap();
    let committed = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(MANIFEST);
    fs::copy(committed, root.join(MANIFEST)).unwrap();
    dir
}

fn put(root: &Path, name: &str, text: &str) {
    let path = root.join(DIR).join(name);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn unit_compat_tools_reports_conform_or_say_why() {
    let dir = work_tree("reports-conform");
    let root = dir.path();
    assert_eq!(
        run(root, false).unwrap(),
        Vec::<String>::new(),
        "no reports"
    );

    let good = real("claude", "2.1.250", CLAUDE_ACP);
    put(
        root,
        "claude/good.json",
        &serde_json::to_string_pretty(&good).unwrap(),
    );
    let mut stray = json!(good);
    stray["scenarios"][0]["stray"] = true.into();
    put(root, "stray.json", &stray.to_string());
    let mut head = good.clone();
    head.commit = "HEAD".into();
    put(root, "head.json", &serde_json::to_string(&head).unwrap());
    put(root, "notes.txt", "not a report");
    let problems = run(root, false).unwrap();
    assert_eq!(problems.len(), 2, "{problems:#?}");
    let head = "testdata/compat/reports/head.json: the commit is not a full hash";
    assert_eq!(problems[0], head);
    assert!(problems[1].starts_with("testdata/compat/reports/stray.json: unknown field `stray`"));

    fs::write(root.join(report::SCHEMA), "{}\n").unwrap();
    let problems = run(root, false).unwrap();
    assert_eq!(
        problems[0],
        format!(
            "{} is stale: UPDATE_GOLDEN=1 cargo test -p xtask",
            report::SCHEMA
        )
    );
}

#[test]
fn unit_compat_tools_range_is_the_baseline_widened_by_counted_reports() {
    let baseline = ["2.1.200-2.1.296", "0.150.1-0.163.0"];
    assert_eq!(
        ranges(Vec::new()),
        (baseline.map(Into::into).into(), Vec::new())
    );

    use Outcome::{Fail, Inconclusive, Pass};
    let counted = vec![
        real("claude", "2.1.1000", CLAUDE_ACP),
        report(Mode::IsolatedConfig, "codex", "0.149.0", CODEX_ACP, &[Pass]),
    ];
    let left_out = vec![
        report(Mode::Fake, "claude", "fake", CLAUDE_ACP, &[Pass]),
        real("claude", "2.1.2000", "0.90.0"),
        report(
            Mode::RealConfig,
            "claude",
            "2.1.3000",
            CLAUDE_ACP,
            &[Pass, Fail],
        ),
        report(
            Mode::RealConfig,
            "claude",
            "2.1.4000",
            CLAUDE_ACP,
            &[Inconclusive, Pass],
        ),
        report(Mode::RealConfig, "codex", "0.1.0", CODEX_ACP, &[]),
    ];
    let wider = ["2.1.200-2.1.1000", "0.149.0-0.163.0"];
    let all = [counted, left_out].concat();
    assert_eq!(ranges(all), (wider.map(Into::into).into(), Vec::new()));

    let wrong = vec![
        real("claude", "2.1", CLAUDE_ACP),
        real("gemini", "1.0.0", "1.0.0"),
    ];
    let (spans, problems) = ranges(wrong);
    assert_eq!(spans, baseline);
    assert_eq!(
        problems,
        [
            "r.json: the CLI version `2.1` is not x.y.z",
            "r.json: the CLI `gemini` is neither claude nor codex"
        ]
    );
    for text in ["+1.0.0", "1.0.0.0", "1..0", "1.0.x", ""] {
        assert_eq!(version(text), None, "{text}");
    }
}

#[test]
fn unit_compat_tools_manifest_holds_the_range_the_reports_give() {
    let dir = work_tree("reports-manifest");
    let root = dir.path();
    let before = Manifest::load(&root.join(MANIFEST)).unwrap();
    let wider = real("claude", "2.1.1000", &before.claude_agent_acp.version);
    put(root, "wider.json", &serde_json::to_string(&wider).unwrap());
    let problems = run(root, false).unwrap();
    assert_eq!(
        problems,
        [
            "runtime/manifest.json: cli.claude.verified is 2.1.200-2.1.296, the reports give \
          2.1.200-2.1.1000: cargo xtask compat-reports --write"
        ]
    );

    // Nothing is written while a report is wrong.
    put(root, "broken.json", "{}");
    assert_eq!(run(root, true).unwrap().len(), 1);
    assert_eq!(Manifest::load(&root.join(MANIFEST)).unwrap(), before);
    fs::remove_file(root.join(DIR).join("broken.json")).unwrap();

    assert_eq!(run(root, true).unwrap(), Vec::<String>::new());
    let mut want = before;
    want.cli.claude.verified.max = "2.1.1000".into();
    assert_eq!(Manifest::load(&root.join(MANIFEST)).unwrap(), want);
    assert_eq!(run(root, false).unwrap(), Vec::<String>::new());
}
