//! `ui`: the screenshot regression, `pnpm test:ui` (design 5.1.6, 5.9.1). It
//! holds the machine lock. In `ci pr` it runs only when the commits since
//! `main` touch a path the screenshots depend on (`UI_PATHS`), or when there
//! is nothing to compare; `ci ui` and `ci nightly` always run it. The run
//! never writes a baseline: only `pnpm ui:baseline` does.

use anyhow::Result;

use crate::ci::ctx::JobCtx;
use crate::ci::git;

/// What the screenshots are built from: a directory (ending in `/`) or a file.
pub const UI_PATHS: &[&str] = &[
    "packages/ui-core/",
    "packages/shell-api/",
    "packages/host-client/",
    "packages/protocol/",
    "packages/i18n/",
    "protocol/fixtures/",
    "tools/ui-screenshots/",
    "package.json",
    "pnpm-lock.yaml",
    "tsconfig.base.json",
];

const SUITE: &str = "tools/ui-screenshots";

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    if ctx.target == "pr" && git::untouched_since_main(ctx.root, UI_PATHS)? {
        ctx.skip(
            "test-ui",
            "no path the screenshots depend on changed since main",
        );
        return Ok(());
    }
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    // Playwright's pinned Chromium; nothing to fetch once it is there.
    let mut browser = ctx.command("pnpm");
    browser.args(["--dir", SUITE, "exec", "playwright", "install", "chromium"]);
    ctx.check("browser", &mut browser)?;

    let mut test = ctx.command("pnpm");
    test.args(["--dir", SUITE, "exec", "playwright", "test"])
        .env_remove("CUPPER_UI_BASELINE");
    ctx.playwright("test-ui", &mut test)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_paths_take_the_ui_packages_and_the_root_manifests() {
        for path in [
            "packages/ui-core/src/shell/Shell.tsx",
            "packages/protocol/src/index.ts",
            "protocol/fixtures/methods.json",
            "tools/ui-screenshots/baselines/shell-light.png",
            "pnpm-lock.yaml",
            "package.json",
        ] {
            assert!(git::touches(UI_PATHS, path), "{path}");
        }
        for path in [
            "crates/cupper-host/src/lib.rs",
            "apps/desktop/package.json",
            "docs/tasks/T118-ui-screenshots/notes.md",
            "packages/ui-core-extra/x.ts",
        ] {
            assert!(!git::touches(UI_PATHS, path), "{path}");
        }
    }
}
