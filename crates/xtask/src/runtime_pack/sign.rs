//! Re-signing the cached Node copy with cupper's identity (ADR-08, design
//! 5.10.2): hardened runtime, `allow-jit` as the only entitlement, the
//! identifier `dev.cupper.runtime.node`. Ad-hoc signatures carry no timestamp,
//! so the result is the same bytes on every machine. Only the copy under
//! `target/` is touched, never an installed Node.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Signs `binary` ad hoc and returns its cdhash (40 hex digits).
pub fn sign(binary: &Path, identifier: &str, entitlements: &Path) -> Result<String> {
    let mut sign = Command::new("codesign");
    sign.args([
        "--force",
        "--sign",
        "-",
        "--options",
        "runtime",
        "--timestamp=none",
    ])
    .args(["--identifier", identifier, "--entitlements"])
    .arg(entitlements)
    .arg(binary);
    codesign(&mut sign)?;
    codesign(
        Command::new("codesign")
            .args(["--verify", "--strict"])
            .arg(binary),
    )?;
    let details = codesign(
        Command::new("codesign")
            .arg("--display")
            .arg("--verbose=3")
            .arg(binary),
    )?;
    details
        .lines()
        .find_map(|line| line.strip_prefix("CDHash="))
        .map(str::to_owned)
        .context("codesign printed no CDHash")
}

/// Runs codesign and returns what it printed on stderr, where it reports.
fn codesign(cmd: &mut Command) -> Result<String> {
    let output = cmd
        .stdin(Stdio::null())
        .output()
        .context("cannot run codesign")?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        bail!("codesign failed: {}", stderr.trim());
    }
    Ok(stderr)
}
