//! `license`: `cargo deny` over the Rust dependency graph (ADR-11). The npm and
//! runtime license gate (`tools/license-gate`) joins later (design 5.1.6).

use anyhow::Result;

use crate::ci::ctx::JobCtx;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    let mut deny = ctx.cargo();
    deny.args(["deny", "--locked", "check"]);
    ctx.check("cargo-deny", &mut deny)
}
