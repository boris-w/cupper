//! `compat-framework` (design 5.1.6): the compat suite's framework, so that
//! the code only the maintainer runs with the real CLIs does not rot. The
//! unit tests of `xtask compat`, then `xtask compat --fake`; no real CLI runs.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut unit = ctx.cargo();
    unit.args(["test", "-p", "xtask", "--locked", "--", "compat::"]);
    ctx.libtest("test-compat", &mut unit)?;
    let mut fake = ctx.command(std::env::current_exe()?);
    let out = ctx.step_file("compat-fake", "out");
    fake.args(["compat", "--fake", "--out"]).arg(out);
    ctx.check("compat-fake", &mut fake)
}
