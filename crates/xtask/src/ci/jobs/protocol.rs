//! `protocol` (design 5.1.6, 5.3.10, 5.12.5): the committed contract is what
//! the Rust types generate, and the shared fixtures pass in Rust and TS, the
//! views included. `pnpm gen` exports `protocol/schema/v1.json` and
//! regenerates `packages/protocol`; anything it changes fails the job and
//! stays in the work tree, ready to be committed. The contract only adds to
//! what M1a froze: `tools/schema-compat` compares the schema with
//! `protocol/released/v1-m1a.json` by direction and checks its integers, and
//! the ShellAPI's build checks it against `packages/shell-api/api-m1a.d.ts`.

use anyhow::{Context, Result, bail};

use crate::ci::ctx::JobCtx;

/// The schema as M1a froze it.
const RELEASED: &str = "protocol/released/v1-m1a.json";

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

    // UNIT-proto-fixtures and UNIT-views-parity, then UNIT-ts-fixtures and
    // the TS views on the same files.
    let mut rust = ctx.cargo();
    rust.args([
        "test",
        "-p",
        "cupper-protocol",
        "--locked",
        "--no-fail-fast",
    ])
    .args(["--", "unit_proto_fixtures", "unit_views_parity"]);
    failures.extend(ctx.libtest("fixtures-rs", &mut rust).err());
    let mut ts = ctx.command("pnpm");
    ts.args(["--dir", "packages/protocol", "run", "test"]);
    failures.extend(ctx.vitest("fixtures-ts", &mut ts).err());
    let mut views = ctx.command("pnpm");
    views.args([
        "--dir",
        "packages/host-client",
        "run",
        "test",
        "src/views/parity.test.ts",
    ]);
    failures.extend(ctx.vitest("views-ts", &mut views).err());

    // UNIT-schema-compat's rules on the frozen contract (T016).
    let compat = "tools/schema-compat/src/main.ts";
    let mut schema = ctx.command("node");
    schema.args([compat, RELEASED, "protocol/schema/v1.json"]);
    failures.extend(ctx.check("schema-compat", &mut schema).err());
    let mut integers = ctx.command("node");
    integers.args([compat, "--integers", "protocol/schema/v1.json"]);
    failures.extend(ctx.check("integers", &mut integers).err());
    let mut shell = ctx.command("pnpm");
    shell.args(["exec", "tsc", "--build", "packages/shell-api"]);
    failures.extend(ctx.check("shell-api-m1a", &mut shell).err());

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
