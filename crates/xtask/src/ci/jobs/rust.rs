//! `rust`: formatting, clippy with warnings denied, and the workspace tests
//! (design 5.1.6). Later tasks add the feature matrix, the iOS build of
//! `cupper-protocol` and the golden tests.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut fmt = ctx.cargo();
    fmt.args(["fmt", "--all", "--check"]);
    ctx.check("fmt", &mut fmt)?;

    let mut clippy = ctx.cargo();
    clippy.args([
        "clippy",
        "--workspace",
        "--all-targets",
        "--all-features",
        "--locked",
        "--",
        "-D",
        "warnings",
    ]);
    ctx.check("clippy", &mut clippy)?;

    let mut test = ctx.cargo();
    test.args(["test", "--workspace", "--all-features", "--locked"]);
    ctx.libtest("test", &mut test)
}
