//! `testdata-scan` (design 5.1.6): `tools/trace-ids` checks the test ID
//! registry `tests/ids.toml` against the plan and the design (5.9.10). The
//! personal data scan of `testdata/` joins with T052, the compat report checks
//! with T064.

use anyhow::Result;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;

    let mut trace_ids = ctx.command("node");
    trace_ids.args(["tools/trace-ids/src/main.ts", "check"]);
    ctx.check("trace-ids", &mut trace_ids)
}
