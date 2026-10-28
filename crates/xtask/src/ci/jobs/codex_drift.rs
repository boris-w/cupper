//! `codex-drift` (nightly, design 5.1.6, ADR-08): whether the newest codex on
//! npm changed the app-server methods the pinned codex-acp uses, against the
//! baseline in `testdata/codex-drift/` (`tools/codex-drift`).
//!
//! Fetching from npm is a step of its own, tried as often as the license job
//! fetches its advisory database. When every try fails, the drift is
//! undetermined, never a drift: both steps are skipped, the summary saying
//! why. The comparison runs offline; a drift fails the job unless it is
//! the one reviewed for that codex version (`testdata/codex-drift/reviewed.json`).

use std::process::Command;

use anyhow::Result;

use super::license::{FETCHES, retry};
use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let dir = ctx.root.join("target").join("codex-drift");
    let tool = |ctx: &JobCtx, command: &str| -> Command {
        let mut node = ctx.command("node");
        node.args(["tools/codex-drift/src/main.ts", command])
            .arg(&dir);
        node
    };
    if !retry(FETCHES, || ctx.attempt("fetch", &mut tool(ctx, "fetch")))? {
        let reason =
            format!("npm could not be reached in {FETCHES} tries: the drift is undetermined");
        ctx.skip("fetch", &reason);
        ctx.skip("drift", &reason);
        return Ok(());
    }
    ctx.passed += 1;
    ctx.check("drift", &mut tool(ctx, "check"))
}
