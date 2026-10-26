//! `compat-framework` (design 5.1.6): the compat suite's framework, so that
//! the code only the maintainer runs with the real CLIs does not rot. The
//! unit tests of `xtask compat`, then `xtask compat --fake`; no real CLI runs.
//! In `ci pr` it runs only when the commits since `main` touch the suite's
//! own code (`COMPAT_PATHS`), or when there is nothing to compare; every
//! night it always runs.

use anyhow::Result;

use crate::ci::ctx::JobCtx;
use crate::ci::git;

/// The compat suite's own code: the framework and its report, the job, the
/// runner and its scenarios, the schema and the reports. The fake agent and
/// the host are left out: nearly every PR changes them, and the nightly run
/// covers them.
const COMPAT_PATHS: &[&str] = &[
    "crates/xtask/src/compat/",
    "crates/xtask/src/ci/jobs/compat_framework.rs",
    "crates/cupper-host/tests/compat.rs",
    "crates/cupper-host/tests/compat/",
    "testdata/compat/",
];

const STEPS: [&str; 2] = ["test-compat", "compat-fake"];

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    if ctx.target == "pr" && git::untouched_since_main(ctx.root, COMPAT_PATHS)? {
        for step in STEPS {
            ctx.skip(step, "no path of the compat suite changed since main");
        }
        return Ok(());
    }
    let mut unit = ctx.cargo();
    unit.args(["test", "-p", "xtask", "--locked", "--", "compat::"]);
    ctx.libtest(STEPS[0], &mut unit)?;
    let mut fake = ctx.command(std::env::current_exe()?);
    let out = ctx.step_file(STEPS[1], "out");
    fake.args(["compat", "--fake", "--out"]).arg(out);
    ctx.check(STEPS[1], &mut fake)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{TempDir, clean_repo, git as run_git};

    /// A branch off `main` with one commit that adds `path`.
    fn branch_adding(name: &str, path: &str) -> TempDir {
        let repo = clean_repo(name);
        let dir = repo.path();
        run_git(dir, &["branch", "-M", "main"]);
        run_git(dir, &["checkout", "--quiet", "-b", "task"]);
        let file = dir.join(path);
        fs::create_dir_all(file.parent().expect("a parent")).expect("mkdir");
        fs::write(&file, "changed\n").expect("write");
        run_git(dir, &["add", path]);
        run_git(dir, &["commit", "--quiet", "-m", "task"]);
        repo
    }

    #[test]
    fn unit_compat_tools_ci_pr_runs_the_framework_only_for_its_paths() {
        let elsewhere = branch_adding("compat-elsewhere", "crates/cupper-host/src/lib.rs");
        let logs = TempDir::new("compat-elsewhere-logs");
        let mut ctx = JobCtx::new(elsewhere.path(), logs.path(), "compat-framework");
        ctx.target = "pr";
        run(&mut ctx).expect("skipped");
        assert_eq!(ctx.skipped, STEPS);
        assert_eq!((ctx.passed, ctx.failed), (0, 0));

        let scenarios = "crates/cupper-host/tests/compat/scenarios.rs";
        let suite = branch_adding("compat-suite", scenarios);
        assert!(!git::untouched_since_main(suite.path(), COMPAT_PATHS).expect("git"));
        for path in [
            "crates/xtask/src/compat/report.rs",
            "testdata/compat/reports/a.json",
        ] {
            assert!(git::touches(COMPAT_PATHS, path), "{path}");
        }
        for path in [
            "crates/cupper-fake-agent/src/main.rs",
            "crates/cupper-host/tests/compat.rsx",
        ] {
            assert!(!git::touches(COMPAT_PATHS, path), "{path}");
        }
    }
}
