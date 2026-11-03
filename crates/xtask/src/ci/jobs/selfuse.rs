//! `selfuse` (nightly, T160): builds Cupper Local, the self-use build of
//! design 5.7.9, and checks the very app it built (`apps/desktop/selfuse`):
//! IT-selfuse-build; E2E-selfuse-coexist, beside the dev build; and
//! E2E-selfuse-replace, with a next self-use build whose host and CLI come
//! from the dev profile, so that their cdhashes differ. The apps run on their
//! real data directories, so the job runs on GitHub-hosted runners only: on
//! the author's Mac, `cupper-local` holds the self-use build in daily use.
//! What the apps logged goes next to the step logs.

use std::path::Path;

use anyhow::Result;

use super::e2e;
use crate::ci::ctx::{Artifact, JobCtx};
use crate::ci::facts;

const APP: &str = "apps/desktop/dist/pack/local/mac-arm64/Cupper Local.app";
const NEXT: &str = "apps/desktop/dist/pack/local-next/mac-arm64/Cupper Local.app";
const DEV: &str = "apps/desktop/dist/pack/dev/mac-arm64/cupper Dev.app";
const DESKTOP: &str = "--dir=apps/desktop";
const LOCAL: &str = "--config=electron-builder.local.yml";

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    if !facts::hosted() {
        let why = "its apps take the real cupper-local; GitHub-hosted runners only";
        ctx.skip("test", why);
        return Ok(());
    }
    e2e::prepare(ctx)?;
    let pnpm = |args: &[&str]| {
        let mut cmd = ctx.command("pnpm");
        cmd.args(args);
        cmd
    };
    let mut host = ctx.cargo();
    host.args(["xtask", "host-bundle", "--kind", "self-use", "--debug"]);
    let next = "-c.directories.output=dist/pack/local-next";
    let builds = [
        ("build", pnpm(&["build:local"])),
        // The next build: the same app around a host and a CLI of the dev profile.
        ("host-next", host),
        (
            "pack-next",
            pnpm(&[DESKTOP, "exec", "electron-builder", LOCAL, next]),
        ),
        ("build-dev", pnpm(&[DESKTOP, "run", "build:dev"])),
    ];
    for (step, mut build) in builds {
        ctx.check(step, &mut build)?;
    }
    let sha256 = e2e::digest(&ctx.root.join(APP))?;
    ctx.artifacts.push(Artifact {
        name: "Cupper Local.app".to_owned(),
        sha256,
    });

    let mut test = ctx.command("caffeinate");
    test.args(["-d", "pnpm", DESKTOP, "run", "test:selfuse"])
        .env("CUPPER_SELFUSE_APP", ctx.root.join(APP))
        .env("CUPPER_SELFUSE_NEXT_APP", ctx.root.join(NEXT))
        .env("CUPPER_DEV_APP", ctx.root.join(DEV));
    let tested = ctx.vitest("test", &mut test);
    let logs = Path::new(&std::env::var_os("HOME").unwrap_or_default()).join("Library/Logs");
    for name in ["cupper-local", "cupper-dev"] {
        let mut copy = ctx.command("ditto");
        copy.arg(logs.join(name)).arg(ctx.step_file(name, "logs"));
        ctx.attempt(&format!("logs-{name}"), &mut copy)?;
    }
    tested
}
