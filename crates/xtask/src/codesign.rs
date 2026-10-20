//! Ad-hoc code signing with the system `codesign` (design 5.10.2). Ad-hoc
//! signatures carry no timestamp, so signing the same file the same way gives
//! the same bytes on every machine. Nothing here uses the keychain.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

pub struct Signing<'a> {
    pub identifier: &'a str,
    pub hardened_runtime: bool,
    pub entitlements: Option<&'a Path>,
}

/// Signs `path`, a Mach-O or a bundle, ad hoc; verifies it, nested code
/// included; returns its cdhash.
pub fn sign(path: &Path, signing: &Signing) -> Result<String> {
    let mut sign = Command::new("codesign");
    sign.args(["--force", "--sign", "-", "--timestamp=none"])
        .args(["--identifier", signing.identifier]);
    if signing.hardened_runtime {
        sign.args(["--options", "runtime"]);
    }
    if let Some(entitlements) = signing.entitlements {
        sign.arg("--entitlements").arg(entitlements);
    }
    codesign(sign.arg(path))?;
    codesign(
        Command::new("codesign")
            .args(["--verify", "--strict", "--deep"])
            .arg(path),
    )?;
    cdhash(path)
}

/// The cdhash (40 hex digits) of the code at `path`; for a bundle, of its
/// main executable.
pub fn cdhash(path: &Path) -> Result<String> {
    let details = codesign(
        Command::new("codesign")
            .arg("--display")
            .arg("--verbose=3")
            .arg(path),
    )?;
    details
        .lines()
        .find_map(|line| line.strip_prefix("CDHash="))
        .map(str::to_owned)
        .with_context(|| format!("codesign printed no CDHash for {}", path.display()))
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
