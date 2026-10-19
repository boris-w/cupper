//! `differential` (nightly, design 5.9.1): random host streams fold into the
//! same JSON in the Rust views and the TS views (UNIT-views-differential).
//! Rust's proptest writes the streams and its views to a file next to the
//! step logs; the host-client folds them in Node.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    let cases = ctx.step_file("cases", "json");
    let mut rust = ctx.cargo();
    rust.args(["test", "-p", "cupper-protocol", "--locked", "--lib"])
        .args(["--", "--ignored", "unit_views_differential"])
        .env("CUPPER_VIEW_CASES", &cases);
    ctx.libtest("cases-rs", &mut rust)?;
    let mut ts = ctx.command("pnpm");
    ts.args(["--dir", "packages/host-client", "run", "test"])
        .arg("src/views/differential.test.ts")
        .env("CUPPER_VIEW_CASES", &cases);
    ctx.vitest("views-ts", &mut ts)
}
