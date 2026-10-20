//! claude-agent-acp's npm tree, downloaded by its lockfile (ADR-08, ADR-11,
//! design 5.2.12). Each package's tarball comes from the registry and is
//! checked against the lockfile's SRI before it is unpacked; only regular
//! files are written, each below its package's directory. No package script
//! runs, and the optional packages, which carry the Claude Code binaries, are
//! never downloaded.

use std::fs::{DirBuilder, File};
use std::io;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Component, Path, PathBuf};
use std::pin::pin;
use std::time::Duration;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use flate2::read::GzDecoder;
use futures::{TryStreamExt as _, stream};
use serde_json::Value;
use sha2::{Digest as _, Sha512};
use tar::{Archive, EntryType};

use super::Reason;
use crate::clock::Clock;

/// The one registry the installer downloads from, unless a test injects its
/// own (design 5.9.9).
pub const REGISTRY: &str = "https://registry.npmjs.org/";

/// Downloads at once (T115 ruling).
const CONCURRENT: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    /// Where it is installed: `node_modules/<name>`, maybe nested.
    pub key: String,
    pub url: String,
    pub integrity: String,
}

/// The packages an install without optional and dev dependencies downloads,
/// from `registry` (ending in `/`) in place of [`REGISTRY`]. A lockfile with
/// any other source or an unsafe install path is not accepted.
pub fn packages(lockfile: &[u8], registry: &str) -> Result<Vec<Package>, Reason> {
    let lock: Value = serde_json::from_slice(lockfile).map_err(|_| Reason::HashMismatch)?;
    let entries = lock["packages"].as_object().ok_or(Reason::HashMismatch)?;
    let mut packages = Vec::new();
    for (key, entry) in entries {
        let skipped = ["optional", "dev", "devOptional", "link"];
        if key.is_empty() || skipped.iter().any(|flag| entry[flag] == true) {
            continue;
        }
        let safe = Path::new(key)
            .components()
            .all(|c| matches!(c, Component::Normal(_)));
        let resolved = entry["resolved"]
            .as_str()
            .and_then(|url| url.strip_prefix(REGISTRY));
        let integrity = entry["integrity"].as_str();
        let (Some(path), Some(integrity), true) = (resolved, integrity, safe) else {
            return Err(Reason::HashMismatch);
        };
        if !key.starts_with("node_modules/") {
            return Err(Reason::HashMismatch);
        }
        packages.push(Package {
            key: key.clone(),
            url: format!("{registry}{path}"),
            integrity: integrity.to_owned(),
        });
    }
    Ok(packages)
}

/// The installer's HTTP client: its own User-Agent, nothing about the user
/// (design 5.2.12), and `proxy`, the login shell's `HTTPS_PROXY`, as its only
/// proxy. Redirects are not followed: only the registry is asked.
pub fn client(proxy: Option<&str>) -> Result<reqwest::Client, Reason> {
    let agent = concat!("cupper-runtime-installer/", env!("CARGO_PKG_VERSION"));
    let builder = reqwest::Client::builder().user_agent(agent).no_proxy();
    let builder = builder.redirect(reqwest::redirect::Policy::none());
    let builder = match proxy {
        Some(url) => builder.proxy(reqwest::Proxy::https(url).map_err(|_| Reason::Network)?),
        None => builder,
    };
    builder.build().map_err(|_| Reason::Network)
}

/// Downloads and unpacks every package below `root`, [`CONCURRENT`] at a
/// time, each within `limit` on `clock`; the first failure ends them all.
pub async fn fetch(
    client: &reqwest::Client,
    clock: &dyn Clock,
    limit: Duration,
    packages: &[Package],
    root: &Path,
) -> Result<(), Reason> {
    let packages = stream::iter(packages.iter().map(Ok));
    packages
        .try_for_each_concurrent(CONCURRENT, |package| async move {
            let tarball = download(client, clock, limit, package).await?;
            let dest = root.join(&package.key);
            let unpacked = tokio::task::spawn_blocking(move || unpack(&tarball, &dest)).await;
            unpacked.map_err(|_| Reason::Disk)?
        })
        .await
}

async fn download(
    client: &reqwest::Client,
    clock: &dyn Clock,
    limit: Duration,
    package: &Package,
) -> Result<Vec<u8>, Reason> {
    let get = async {
        let response = client.get(&package.url).send().await.ok()?;
        let ok = response.status().is_success();
        let bytes = response.bytes().await.ok()?;
        ok.then_some(bytes)
    };
    let bytes = tokio::select! {
        bytes = get => bytes.ok_or(Reason::Network)?,
        () = pin!(clock.timer_at(clock.now_mono() + limit)) => return Err(Reason::Network),
    };
    match sri_matches(&package.integrity, &bytes) {
        true => Ok(bytes.into()),
        false => Err(Reason::HashMismatch),
    }
}

/// `sha512-<base64>`, the form npm writes; other algorithms are not trusted.
fn sri_matches(integrity: &str, bytes: &[u8]) -> bool {
    let digest = STANDARD.encode(Sha512::digest(bytes));
    integrity
        .split_whitespace()
        .any(|sri| sri.strip_prefix("sha512-") == Some(digest.as_str()))
}

/// Writes the regular files of a gzipped tarball below `dest`, without the
/// archive's top directory (`package/`).
fn unpack(tarball: &[u8], dest: &Path) -> Result<(), Reason> {
    let unreadable = |_| Reason::HashMismatch;
    let mut archive = Archive::new(GzDecoder::new(tarball));
    let mut dirs = DirBuilder::new();
    dirs.recursive(true).mode(0o700);
    for entry in archive.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        let kind = entry.header().entry_type();
        if !matches!(kind, EntryType::Regular | EntryType::Continuous) {
            continue;
        }
        let path = entry.path().map_err(unreadable)?;
        let path = inner_path(&path).ok_or(Reason::HashMismatch)?;
        let target = dest.join(path);
        let parent = target.parent().unwrap_or(dest);
        dirs.create(parent).map_err(disk)?;
        let mut file = File::create(&target).map_err(disk)?;
        io::copy(&mut entry, &mut file).map_err(disk)?;
    }
    Ok(())
}

fn disk(_: io::Error) -> Reason {
    Reason::Disk
}

/// The path below the top directory: plain names only, no `..` or root.
fn inner_path(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components().filter(|part| *part != Component::CurDir);
    let Some(Component::Normal(_)) = parts.next() else {
        return None;
    };
    let inner: PathBuf = parts
        .map(|part| match part {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect::<Option<_>>()?;
    (!inner.as_os_str().is_empty()).then_some(inner)
}
