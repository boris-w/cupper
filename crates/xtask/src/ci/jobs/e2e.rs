//! `e2e`: Playwright drives the e2e build of the app with its real host, the
//! fake agent and the CLI, `tools/e2e` (design 5.1.6, 5.9.4). It holds the
//! machine lock. The app, its `CupperHost.app` and the e2e CLI are built from
//! the current commit every time, and the summary records their digests. In
//! `ci pr` it runs the `@smoke` tests, and only when the commits since `main`
//! touch a path the app is built from (`E2E_PATHS`) or there is nothing to
//! compare; `ci e2e` and `ci nightly` run every test. Either way those tagged
//! `@focus` run last and one at a time: they need the window's focus, which
//! another test's app takes when it starts.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::ci::ctx::{Artifact, JobCtx};
use crate::ci::git;

/// What the e2e build and the suite are made of: a directory (ending in `/`),
/// a file, or a prefix of crate directories.
pub const E2E_PATHS: &[&str] = &[
    "apps/desktop/",
    "packages/",
    "crates/cupper-",
    "runtime/",
    "tools/e2e/",
    "crates/xtask/src/host_bundle.rs",
    "crates/xtask/src/runtime_pack/",
    "crates/xtask/src/ci/jobs/e2e.rs",
    "Cargo.toml",
    "Cargo.lock",
    "package.json",
    "pnpm-lock.yaml",
    "pnpm-workspace.yaml",
    "tsconfig.base.json",
];

const SUITE: &str = "tools/e2e";
/// Built by `cargo xtask runtime-pack`, which `host-bundle` takes the runtime from.
const RUNTIME: &str = "target/runtime-pack/Contents";
const ARTIFACTS: &[(&str, &str)] = &[
    (
        "Cupper E2E.app",
        "apps/desktop/dist/pack/e2e/mac-arm64/Cupper E2E.app",
    ),
    ("CupperHost.app", "target/host-bundle/e2e/CupperHost.app"),
    ("cupper", "target/host-bundle/e2e/cupper"),
];

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let smoke = ctx.target == "pr";
    if smoke
        && let Some(changed) = git::changed_since_main(ctx.root)?
        && !changed.iter().any(|path| touches_e2e(path))
    {
        ctx.skip(
            "test-e2e",
            "no path the e2e build depends on changed since main",
        );
        return Ok(());
    }
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    if !ctx.root.join(RUNTIME).is_dir() {
        // Public downloads only, pinned by `runtime/manifest.json`.
        let mut pack = ctx.cargo();
        pack.args(["xtask", "runtime-pack"]);
        ctx.check("runtime-pack", &mut pack)?;
    }
    let mut agent = ctx.cargo();
    agent.args(["build", "--locked", "--package", "cupper-fake-agent"]);
    ctx.check("fake-agent", &mut agent)?;
    let mut build = ctx.command("pnpm");
    build.args(["build:e2e"]);
    ctx.check("build", &mut build)?;
    for (name, path) in ARTIFACTS {
        let sha256 = digest(&ctx.root.join(path))?;
        ctx.artifacts.push(Artifact {
            name: (*name).to_owned(),
            sha256,
        });
    }

    // The display stays awake while the suite runs: once it sleeps the
    // screen locks, and the app's windows can be neither seen nor focused.
    let mut suite = |step: &str, args: &[&str]| {
        let mut test = ctx.command("caffeinate");
        test.args(["-d", "pnpm", "--dir", SUITE, "exec", "playwright", "test"])
            .args(args);
        ctx.playwright(step, &mut test)
    };
    let (rest, focus): (&[&str], &str) = if smoke {
        let both = "(?=.*@smoke)(?=.*@focus)";
        (&["--grep", "@smoke", "--grep-invert", "@focus"], both)
    } else {
        (&["--grep-invert", "@focus"], "@focus")
    };
    let rest = suite("test-e2e", rest);
    let focus = suite("test-e2e-focus", &["--grep", focus, "--workers", "1"]);
    rest.and(focus)
}

fn touches_e2e(path: &str) -> bool {
    E2E_PATHS.iter().any(|e2e| {
        if e2e.ends_with('/') || e2e.ends_with('-') {
            path.starts_with(e2e)
        } else {
            path == *e2e
        }
    })
}

/// The sha-256 of a file, or of a bundle: each file's path and content and
/// each link's path and target, in path order.
fn digest(path: &Path) -> Result<String> {
    let hex = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    let read =
        |path: &Path| fs::read(path).with_context(|| format!("cannot read {}", path.display()));
    if path.is_file() {
        return Ok(hex(&read(path)?));
    }
    let mut entries = Vec::new();
    let mut pending = vec![path.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("cannot list {}", dir.display()))? {
            let entry = entry?.path();
            let name = entry.strip_prefix(path)?.to_string_lossy().into_owned();
            let kind = fs::symlink_metadata(&entry)?.file_type();
            if kind.is_symlink() {
                entries.push(format!("L {name} {}", fs::read_link(&entry)?.display()));
            } else if kind.is_dir() {
                pending.push(entry);
            } else {
                entries.push(format!("F {name} {}", hex(&read(&entry)?)));
            }
        }
    }
    entries.sort();
    Ok(hex(entries.join("\n").as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::symlink;

    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn unit_ci_e2e_paths_take_what_the_app_is_built_from() {
        for path in [
            "apps/desktop/src/main/index.ts",
            "crates/cupper-host/src/lib.rs",
            "crates/cupper-fake-agent/src/script.rs",
            "packages/ui-core/src/app/App.tsx",
            "runtime/manifest.json",
            "tools/e2e/src/fixture.ts",
            "crates/xtask/src/host_bundle.rs",
            "pnpm-lock.yaml",
        ] {
            assert!(touches_e2e(path), "{path}");
        }
        for path in [
            "docs/tasks/T090-e2e-infra/notes.md",
            "crates/xtask/src/ci/jobs/ui.rs",
            "tools/ui-screenshots/src/shell.spec.ts",
            "tests/ids.toml",
            "crates/cupperx/lib.rs",
        ] {
            assert!(!touches_e2e(path), "{path}");
        }
    }

    #[test]
    fn unit_ci_e2e_bundle_digest_covers_contents_paths_and_links() {
        let dir = TempDir::new("e2e-digest");
        let app = dir.path().join("A.app");
        fs::create_dir_all(app.join("Contents/MacOS")).expect("mkdir");
        fs::write(app.join("Contents/MacOS/a"), "one").expect("write");
        symlink("MacOS/a", app.join("Contents/current")).expect("link");
        let first = digest(&app).expect("digest");
        assert_eq!(digest(&app).expect("again"), first, "stable");

        fs::write(app.join("Contents/MacOS/a"), "two").expect("write");
        let changed = digest(&app).expect("digest");
        assert_ne!(changed, first, "content");
        fs::remove_file(app.join("Contents/current")).expect("unlink");
        symlink("MacOS/b", app.join("Contents/current")).expect("relink");
        assert_ne!(digest(&app).expect("digest"), changed, "link target");

        let file = dir.path().join("cupper");
        fs::write(&file, "cli").expect("write");
        assert_eq!(
            digest(&file).expect("file"),
            format!("{:x}", Sha256::digest(b"cli"))
        );
    }
}
