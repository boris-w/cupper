//! Downloads for `runtime-pack` and `git-lfs`. Only the official release addresses, only
//! https; curl reads no user configuration and sends the project placeholder
//! as its User-Agent, nothing personal (subagent rule 7). Every file is
//! checked against its pinned sha-256 or npm SRI before anything reads it, and
//! again when a cached copy is reused.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use sha2::{Digest, Sha256, Sha512};

const OFFICIAL: &[&str] = &[
    "https://nodejs.org/dist/",
    "https://registry.npmjs.org/",
    "https://github.com/git-lfs/git-lfs/releases/download/",
];

/// What a downloaded file must hash to.
#[derive(Debug, Clone, Copy)]
pub enum Pin<'a> {
    /// Lowercase hex, as in Node's `SHASUMS256.txt`.
    Sha256(&'a str),
    /// npm's Subresource Integrity, `sha512-<base64>`.
    Integrity(&'a str),
}

pub struct Download<'a> {
    pub url: &'a str,
    pub pin: Pin<'a>,
    pub dest: PathBuf,
}

/// Downloads every file that is missing or does not match its pin, in one
/// parallel curl run, then checks every pin.
pub fn fetch(items: &[Download]) -> Result<()> {
    let mut curl = Command::new("curl");
    // `-q` first: no `.curlrc`, which could add headers or credentials.
    curl.args(["-q", "--fail", "--silent", "--show-error", "--location"])
        .args(["--proto", "=https", "--proto-redir", "=https"])
        .args([
            "--retry",
            "3",
            "--retry-all-errors",
            "--connect-timeout",
            "30",
        ])
        .args(["--user-agent", "cupper-research"])
        .args(["--parallel", "--parallel-max", "8"]);
    let mut pending = 0;
    for item in items {
        if !OFFICIAL.iter().any(|prefix| item.url.starts_with(prefix)) {
            bail!("{} is not an official release address", item.url);
        }
        if check(&item.dest, item.pin).is_err() {
            curl.arg("--output").arg(&item.dest).arg(item.url);
            pending += 1;
        }
    }
    if pending > 0 {
        let status = curl
            .stdin(Stdio::null())
            .status()
            .context("cannot run curl")?;
        if !status.success() {
            bail!("curl failed ({status})");
        }
    }
    for item in items {
        check(&item.dest, item.pin).with_context(|| format!("download of {}", item.url))?;
    }
    Ok(())
}

/// Fails unless the file at `path` matches `pin`.
pub fn check(path: &Path, pin: Pin) -> Result<()> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let (got, want) = match pin {
        Pin::Sha256(want) => (sha256_hex(&bytes), want),
        Pin::Integrity(want) => (
            format!("sha512-{}", STANDARD.encode(Sha512::digest(&bytes))),
            want,
        ),
    };
    if got != want {
        bail!("{} hashes to {got}, the pin is {want}", path.display());
    }
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    #[test]
    fn a_file_must_match_its_sha256_or_sri_pin() {
        let dir = TempDir::new("fetch-pin");
        let file = dir.path().join("a.tgz");
        fs::write(&file, "abc").expect("write");
        let sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let sri = "sha512-3a81oZNherrMQXNJriBBMRLm+k6JqX6iCp7u5ktV05ohkpkqJ0/BqDa6PCOj/uu9RU1EI2Q86A4qmslPpUyknw==";
        check(&file, Pin::Sha256(sha256)).expect("sha-256 matches");
        check(&file, Pin::Integrity(sri)).expect("SRI matches");

        fs::write(&file, "abd").expect("write");
        let err = check(&file, Pin::Sha256(sha256)).expect_err("changed file");
        assert!(err.to_string().contains("the pin is"), "{err}");
        check(&file, Pin::Integrity(sri)).expect_err("changed file");
        check(&dir.path().join("missing"), Pin::Sha256(sha256)).expect_err("missing file");
    }

    #[test]
    fn only_official_release_addresses_are_downloaded() {
        let dir = TempDir::new("fetch-official");
        for url in [
            "https://example.com/node.tar.gz",
            "http://registry.npmjs.org/a/-/a-1.0.0.tgz",
            "https://registry.npmjs.org.example.com/a.tgz",
        ] {
            let item = Download {
                url,
                pin: Pin::Sha256("00"),
                dest: dir.path().join("x"),
            };
            let err = fetch(&[item]).expect_err(url);
            assert!(err.to_string().contains("not an official"), "{err}");
        }
    }
}
