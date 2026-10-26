//! `compat-framework` (design 5.1.6): the compat suite's framework, so that
//! the code only the maintainer runs with the real CLIs does not rot. The
//! unit tests of `xtask compat` and of the runner, `xtask compat --fake`, and
//! the plan of a real run (`--real --dry-run`); no real CLI runs. In `ci pr`
//! it runs only when the commits since `main` touch the suite's own code
//! (`COMPAT_PATHS`), or when there is nothing to compare; every night it
//! always runs.

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

const STEPS: [&str; 4] = ["test-compat", "test-runner", "compat-plan", "compat-fake"];

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
    let mut runner = ctx.cargo();
    runner.args("test -p cupper-host --features test-harness --locked --test compat".split(' '));
    ctx.libtest(STEPS[1], &mut runner)?;
    let mut plan = ctx.command(std::env::current_exe()?);
    ctx.check(STEPS[2], plan.args(["compat", "--real", "--dry-run"]))?;
    let mut fake = ctx.command(std::env::current_exe()?);
    let out = ctx.step_file(STEPS[3], "out");
    fake.args(["compat", "--fake", "--out"]).arg(out);
    ctx.check(STEPS[3], &mut fake)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testutil::{TempDir, clean_repo, git as run_git};

    #[test]
    fn unit_compat_tools_ci_pr_runs_the_framework_only_for_its_paths() {
        let paths = [
            ("crates/cupper-host/tests/compat/scenarios.rs", true),
            ("crates/xtask/src/compat/report.rs", true),
            ("testdata/compat/reports/a.json", true),
            ("crates/cupper-fake-agent/src/main.rs", false),
            ("crates/cupper-host/tests/compat.rsx", false),
        ];
        for (path, ours) in paths {
            assert_eq!(git::touches(COMPAT_PATHS, path), ours, "{path}");
        }
        // A branch off main that changes something else: every step skipped.
        let repo = clean_repo("compat-paths");
        let dir = repo.path();
        let commit = |path: &str| {
            fs::create_dir_all(dir.join(path).parent().unwrap()).unwrap();
            fs::write(dir.join(path), "changed\n").unwrap();
            run_git(dir, &["add", path]);
            run_git(dir, &["commit", "--quiet", "-m", path]);
        };
        run_git(dir, &["branch", "-M", "main"]);
        run_git(dir, &["checkout", "--quiet", "-b", "task"]);
        commit("crates/cupper-host/src/lib.rs");
        let logs = TempDir::new("compat-paths-logs");
        let mut ctx = JobCtx::new(dir, logs.path(), "compat-framework");
        ctx.target = "pr";
        run(&mut ctx).unwrap();
        assert_eq!(
            (ctx.skipped, ctx.passed + ctx.failed),
            (STEPS.map(Into::into).into(), 0)
        );
        commit("testdata/compat/reports/a.json");
        assert!(!git::untouched_since_main(dir, COMPAT_PATHS).unwrap());
    }
}
