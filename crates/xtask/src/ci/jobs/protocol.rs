//! `protocol` (design 5.1.6, 5.3.10): the committed contract is what the Rust
//! types generate, and the shared fixtures pass in Rust and TS. `pnpm gen`
//! exports `protocol/schema/v1.json` and regenerates `packages/protocol`;
//! anything it changes fails the job and stays in the work tree, ready to be
//! committed. The comparison with released schemas joins later (T016).

use anyhow::{Context, Result, bail};

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    // Every step runs, so that one report shows a stale contract and the
    // fixtures that fail on it.
    let mut failures = Vec::new();
    let mut generate = ctx.command("pnpm");
    generate.arg("gen");
    let generated = ctx
        .check("gen", &mut generate)
        .and_then(|()| unchanged(ctx));
    failures.extend(generated.err());

    // UNIT-proto-fixtures, then UNIT-ts-fixtures on the same files.
    let mut rust = ctx.cargo();
    rust.args([
        "test",
        "-p",
        "cupper-protocol",
        "--locked",
        "--no-fail-fast",
    ])
    .args(["--", "unit_proto_fixtures"]);
    failures.extend(ctx.libtest("fixtures-rs", &mut rust).err());
    let mut ts = ctx.command("pnpm");
    ts.args(["--dir", "packages/protocol", "run", "test"]);
    failures.extend(ctx.vitest("fixtures-ts", &mut ts).err());

    if !failures.is_empty() {
        let failures: Vec<String> = failures.iter().map(|err| format!("{err:#}")).collect();
        bail!("{}", failures.join("\n  "));
    }
    Ok(())
}

/// Passes when the work tree, clean before the job, still is.
fn unchanged(ctx: &mut JobCtx) -> Result<()> {
    let output = ctx
        .command("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .output()
        .context("cannot run git status")?;
    let changed = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && changed.is_empty() {
        ctx.passed += 1;
        return Ok(());
    }
    ctx.failed += 1;
    bail!("`pnpm gen` changed the work tree; commit the regenerated files:\n{changed}")
}
