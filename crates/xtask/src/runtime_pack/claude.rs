//! The fixed hash of the claude-agent-acp bundle (ADR-08, ADR-11, design
//! 5.10.3). Its npm tree, Anthropic's SDK included, goes into an isolated
//! directory that is no input of anything packaged; it is bundled exactly the
//! way the host bundles it on the user's machine, hashed and deleted.
//! Optional packages, which carry
//! the Claude Code binaries, are never downloaded. Nothing from the tree runs:
//! esbuild-wasm only reads it, run by the re-signed Node with PATH as the only
//! environment variable.

use std::fs;
use std::path::{Component, Path};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use super::fetch::{self, Download, Pin};
use super::unpack;

/// Where the bundle is written, relative to the isolated root.
const OUT: &str = "adapter.mjs";

pub struct Tools<'a> {
    pub node: &'a Path,
    pub script: &'a Path,
    pub esbuild_main: &'a Path,
}

/// The sha-256 of the bundle.
pub fn bundle(isolated: &Path, lockfile: &str, entry: &str, tools: &Tools) -> Result<String> {
    let _cleanup = Cleanup(isolated);
    let _ = fs::remove_dir_all(isolated);
    let downloads = isolated.join(".downloads");
    fs::create_dir_all(&downloads).context("cannot create the isolated directory")?;

    let packages = packages(lockfile)?;
    let items: Vec<Download> = packages
        .iter()
        .enumerate()
        .map(|(index, package)| Download {
            url: &package.resolved,
            pin: Pin::Integrity(&package.integrity),
            dest: downloads.join(format!("{index}.tgz")),
        })
        .collect();
    fetch::fetch(&items)?;
    for (package, item) in packages.iter().zip(&items) {
        unpack::extract(&item.dest, &isolated.join(&package.key))?;
    }
    fs::remove_dir_all(&downloads).context("cannot remove the downloads")?;

    let status = Command::new(tools.node)
        .arg(tools.script)
        .arg(tools.esbuild_main)
        .arg(isolated)
        .arg(entry)
        .arg(OUT)
        .env_clear()
        // esbuild-wasm runs its service as `node` from PATH: only ours.
        .env("PATH", tools.node.parent().unwrap_or(Path::new("/")))
        .current_dir(tools.script.parent().unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .status()
        .context("cannot run the bundle script")?;
    if !status.success() {
        bail!("the bundle script failed ({status})");
    }
    let bytes = fs::read(isolated.join(OUT)).context("cannot read the bundle")?;
    Ok(fetch::sha256_hex(&bytes))
}

/// Removes the isolated directory however the bundle step ends.
struct Cleanup<'a>(&'a Path);

impl Drop for Cleanup<'_> {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(self.0);
    }
}

#[derive(Debug)]
pub struct Package {
    /// The install path, `node_modules/<name>` or nested below another package.
    pub key: String,
    pub resolved: String,
    pub integrity: String,
}

/// The packages of a package-lock (v2 or v3) that an install without optional
/// and dev dependencies downloads.
pub fn packages(lockfile: &str) -> Result<Vec<Package>> {
    let lock: Value = serde_json::from_str(lockfile).context("the lockfile is not JSON")?;
    let entries = lock["packages"]
        .as_object()
        .context("the lockfile has no `packages`")?;
    let mut packages = Vec::new();
    for (key, entry) in entries {
        let skipped = ["optional", "dev", "devOptional", "link"]
            .iter()
            .any(|flag| entry[flag] == true);
        if key.is_empty() || skipped {
            continue;
        }
        let safe = Path::new(key)
            .components()
            .all(|part| matches!(part, Component::Normal(_)));
        let field = |name: &str| {
            entry[name]
                .as_str()
                .map(str::to_owned)
                .with_context(|| format!("`{key}` in the lockfile has no `{name}`"))
        };
        if !key.starts_with("node_modules/") || !safe {
            bail!("unexpected package path `{key}` in the lockfile");
        }
        packages.push(Package {
            key: key.clone(),
            resolved: field("resolved")?,
            integrity: field("integrity")?,
        });
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_dev_and_linked_packages_are_not_downloaded() {
        let lock = r#"{
          "lockfileVersion": 3,
          "packages": {
            "": { "dependencies": { "a": "1.0.0" } },
            "node_modules/a": { "version": "1.0.0", "resolved": "https://registry.npmjs.org/a/-/a-1.0.0.tgz", "integrity": "sha512-a" },
            "node_modules/a/node_modules/b": { "resolved": "https://registry.npmjs.org/b/-/b-2.0.0.tgz", "integrity": "sha512-b" },
            "node_modules/sdk-darwin-arm64": { "optional": true, "resolved": "https://registry.npmjs.org/x.tgz", "integrity": "sha512-x" },
            "node_modules/tool": { "dev": true },
            "node_modules/both": { "devOptional": true },
            "node_modules/local": { "link": true, "resolved": "../local" }
          }
        }"#;
        let keys: Vec<String> = packages(lock)
            .expect("valid lockfile")
            .into_iter()
            .map(|package| package.key)
            .collect();
        assert_eq!(keys, ["node_modules/a", "node_modules/a/node_modules/b"]);
    }

    #[test]
    fn package_paths_must_stay_below_node_modules() {
        for key in ["node_modules/../escape", "elsewhere/a", "/node_modules/a"] {
            let lock = format!(
                r#"{{"packages": {{"{key}": {{"resolved": "https://registry.npmjs.org/a.tgz", "integrity": "sha512-a"}}}}}}"#
            );
            let err = packages(&lock).expect_err(key);
            assert!(
                err.to_string().contains("unexpected package path"),
                "{key}: {err}"
            );
        }
        let lock = r#"{"packages": {"node_modules/a": {"integrity": "sha512-a"}}}"#;
        assert!(packages(lock).is_err(), "no `resolved`");
    }
}
