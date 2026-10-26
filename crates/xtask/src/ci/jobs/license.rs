//! `license` (ADR-11, design 5.1.6): `cargo deny` over the Rust dependency
//! graph, then the first layer of `tools/license-gate` over the app's JS
//! entries, whose metafiles a release build of the app writes, and over the
//! runtime components.
//!
//! `cargo deny` fetches the advisory database first, as a step of its own,
//! and checks offline: a network error is retried, and when every try fails
//! the check runs against the copy of an earlier fetch, the skipped fetch in
//! the summary saying so. An advisory in the database always fails the job.

use std::path::PathBuf;

use anyhow::{Result, bail};

use crate::ci::ctx::JobCtx;

/// How many times fetching the advisory database is tried.
const FETCHES: u32 = 3;

pub fn run(ctx: &mut JobCtx) -> Result<()> {
    advisory_db(ctx)?;
    let mut deny = ctx.cargo();
    deny.args(["deny", "--locked", "--offline", "check"]);
    ctx.check("cargo-deny", &mut deny)?;

    let mut install = ctx.command("pnpm");
    install.args(["install", "--frozen-lockfile"]);
    ctx.check("install", &mut install)?;
    let mut build = ctx.command("pnpm");
    build.args([
        "--filter",
        "@cupper/desktop",
        "exec",
        "vite",
        "build",
        "--mode",
        "release",
    ]);
    ctx.check("metafile", &mut build)?;
    let mut gate = ctx.command("node");
    gate.args(["tools/license-gate/src/main.ts", "check"]);
    ctx.check("license-gate", &mut gate)
}

fn advisory_db(ctx: &mut JobCtx) -> Result<()> {
    let fetched = retry(FETCHES, || {
        let mut fetch = ctx.cargo();
        fetch.args(["deny", "--locked", "fetch", "db"]);
        ctx.attempt("fetch-db", &mut fetch)
    })?;
    if fetched {
        ctx.passed += 1;
    } else if cached_db() {
        let reason = format!(
            "the advisory database could not be fetched in {FETCHES} tries; \
             checking against the copy of an earlier fetch"
        );
        ctx.skip("fetch-db", &reason);
    } else {
        ctx.failed += 1;
        bail!("the advisory database could not be fetched, and there is no earlier copy");
    }
    Ok(())
}

/// Runs `fetch` until it succeeds, `times` at most: whether it did.
fn retry(times: u32, mut fetch: impl FnMut() -> Result<bool>) -> Result<bool> {
    for _ in 0..times {
        if fetch()? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Whether `cargo deny` has fetched an advisory database before (`$CARGO_HOME/advisory-dbs`).
fn cached_db() -> bool {
    let home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    let Some(Ok(entries)) = home.map(|home| home.join("advisory-dbs").read_dir()) else {
        return false;
    };
    entries.filter_map(Result::ok).any(|entry| {
        entry
            .file_name()
            .to_string_lossy()
            .starts_with("advisory-db-")
            && entry.path().is_dir()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advisory_db_fetch_is_tried_until_it_succeeds_three_times_at_most() {
        let mut tries = 0;
        assert!(
            !retry(3, || {
                tries += 1;
                Ok(false)
            })
            .expect("retry")
        );
        assert_eq!(tries, 3);
        let mut tries = 0;
        assert!(
            retry(3, || {
                tries += 1;
                Ok(tries == 2)
            })
            .expect("retry")
        );
        assert_eq!(tries, 2);
    }
}
