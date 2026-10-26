//! `cargo xtask git-lfs` (T186): the git-lfs that IT-git-lfs-snapshot runs,
//! at the pinned version, from git-lfs's official release. The zip is
//! checked against its sha-256 from the release's `sha256sums.asc` before
//! anything reads it, then the program is unpacked to `target/git-lfs/git-lfs`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};

use crate::ci::git;
use crate::runtime_pack::fetch::{self, Download, Pin};

pub const USAGE: &str = "usage: cargo xtask git-lfs\n\n\
    Downloads the pinned git-lfs, checks it and unpacks it to target/git-lfs/git-lfs.";

const VERSION: &str = "3.8.0";

/// The release for this machine, and its sha-256.
fn release() -> Result<(&'static str, &'static str)> {
    Ok(match std::env::consts::ARCH {
        "aarch64" => (
            "https://github.com/git-lfs/git-lfs/releases/download/v3.8.0/git-lfs-darwin-arm64-v3.8.0.zip",
            "caff76a7d070d8160c89bc39b6e85d98f24135b6fed038a3b4de2590d25102d8",
        ),
        "x86_64" => (
            "https://github.com/git-lfs/git-lfs/releases/download/v3.8.0/git-lfs-darwin-amd64-v3.8.0.zip",
            "f1c17aeca0b4eaab9ea606226477dbed3b84b56fe0811a9f967d2ea2b2393c53",
        ),
        arch => bail!("no git-lfs release is pinned for {arch}"),
    })
}

pub fn main(args: &[String]) -> ExitCode {
    if !args.is_empty() {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let outcome = std::env::current_dir()
        .context("no current directory")
        .and_then(|cwd| git::toplevel(&cwd))
        .and_then(|root| install(&root));
    match outcome {
        Ok(program) => {
            println!("{}", program.display());
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("xtask git-lfs: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn install(root: &Path) -> Result<PathBuf> {
    let (url, sha256) = release()?;
    let dir = root.join("target").join("git-lfs");
    fs::create_dir_all(&dir).context("cannot create target/git-lfs")?;
    let zip = dir.join(url.rsplit('/').next().unwrap_or(url));
    let pin = Pin::Sha256(sha256);
    fetch::fetch(&[Download {
        url,
        pin,
        dest: zip.clone(),
    }])?;
    unpack(&zip, pin, &dir)
}

/// Unpacks the program from `zip`, which must match `pin`, into `dir`.
fn unpack(zip: &Path, pin: Pin, dir: &Path) -> Result<PathBuf> {
    fetch::check(zip, pin)?;
    let inner = format!("git-lfs-{VERSION}/git-lfs");
    let status = Command::new("/usr/bin/unzip")
        .args(["-q", "-o"])
        .arg(zip)
        .arg(&inner)
        .arg("-d")
        .arg(dir)
        .stdin(Stdio::null())
        .status()
        .context("cannot run unzip")?;
    if !status.success() {
        bail!("unzip failed ({status})");
    }
    let program = dir.join("git-lfs");
    fs::rename(dir.join(&inner), &program).context("cannot place git-lfs")?;
    Ok(program)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn a_git_lfs_zip_that_does_not_match_its_pin_is_not_unpacked() {
        let dir = TempDir::new("git-lfs-pin");
        let zip = dir.path().join("git-lfs.zip");
        fs::write(&zip, "not the release").expect("write");
        let (_, sha256) = release().expect("a pinned release");
        let err = unpack(&zip, Pin::Sha256(sha256), dir.path()).expect_err("refused");
        assert!(err.to_string().contains("the pin is"), "{err}");
        assert!(!dir.path().join("git-lfs").exists());
    }
}
