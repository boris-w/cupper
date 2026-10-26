//! `security-macos14`: the peer verification tests of `cupper-sys` (design
//! 5.1.6, Q24): IT-sys-peer-current and UNIT-sys-uniqueid. The summary
//! records the macOS they ran on (`os`), and on macOS 14 or 15, the nightly
//! runners', their results count for IT-sys-peer-macos14 or -macos15; on the
//! author's macOS, for IT-sys-peer-current (plan 1.6, 1.8).

use anyhow::{Result, bail};

use crate::ci::ctx::JobCtx;
use crate::ci::facts;

/// libtest name filters; a test runs when its path contains one of them.
const FILTERS: &[&str] = &["it_sys_peer_current", "unit_sys_uniqueid"];

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    eprintln!("  on {}", facts::os(ctx.root));
    let mut test = ctx.cargo();
    test.args([
        "test",
        "-p",
        "cupper-sys",
        "--locked",
        "--no-fail-fast",
        "--",
    ])
    .args(FILTERS);
    ctx.libtest("test", &mut test)?;
    for filter in FILTERS {
        if !ctx.cases.iter().any(|case| case.name.contains(filter)) {
            ctx.failed += 1;
            bail!("no test matching `{filter}` ran");
        }
    }
    Ok(())
}
