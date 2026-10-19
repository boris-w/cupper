//! `cargo xtask runtime-pack [--verify]` (design 5.1.5, 5.10.3): downloads
//! the Node, codex-acp and esbuild-wasm pinned in `runtime/manifest.json` and
//! checks them, re-signs the cached Node copy, and computes the
//! claude-agent-acp bundle hash in an isolated directory. The runtime is staged in
//! `target/runtime-pack/Contents/`, laid out like `CupperHost.app/Contents`
//! (design 5.10.1), for `host-bundle`. Without `--verify` the computed fields
//! are written back to the manifest and the components' license texts to
//! `runtime/licenses/`; with it, any difference fails. Anthropic's SDK and
//! Claude Code never reach the stage or the repository (ADR-11).

mod claude;
mod fetch;
mod manifest;
mod sign;
mod unpack;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, bail};

use crate::ci::git;
use fetch::{Download, Pin};
use manifest::{FileHash, Manifest};

pub const USAGE: &str = "usage: cargo xtask runtime-pack [--verify]\n\n\
    Downloads, checks and stages the bundled runtime in target/runtime-pack/Contents/\n\
    and writes the computed fields of runtime/manifest.json.\n\
    --verify  change nothing; fail unless the runtime matches the manifest";

/// The bundle script, in `runtime/` here and in `Resources/runtime/` in the app.
const SCRIPT: &str = "bundle-claude-acp.mjs";

pub fn main(args: &[String]) -> ExitCode {
    let verify = match args {
        [] => false,
        [flag] if flag == "--verify" => true,
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };
    let outcome = std::env::current_dir()
        .context("no current directory")
        .and_then(|cwd| git::toplevel(&cwd))
        .and_then(|root| run(&root, verify));
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask runtime-pack: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(root: &Path, verify: bool) -> Result<()> {
    let source = root.join("runtime");
    let pinned = Manifest::load(&source.join("manifest.json"))?;
    let cache = root.join("target").join("runtime-pack");
    let downloads = cache.join("downloads");
    let contents = cache.join("Contents");
    if contents.exists() {
        fs::remove_dir_all(&contents).context("cannot clear the stage")?;
    }
    fs::create_dir_all(&downloads).context("cannot create the download cache")?;
    let runtime = contents.join("Resources").join("runtime");

    let tarball = |url: &str| downloads.join(url.rsplit('/').next().unwrap_or(url));
    let (node_tgz, codex_tgz, esbuild_tgz) = (
        tarball(&pinned.node.url),
        tarball(&pinned.codex_acp.url),
        tarball(&pinned.esbuild_wasm.url),
    );
    eprintln!("fetching Node, codex-acp and esbuild-wasm");
    fetch::fetch(&[
        Download {
            url: &pinned.node.url,
            pin: Pin::Sha256(&pinned.node.sha256),
            dest: node_tgz.clone(),
        },
        Download {
            url: &pinned.codex_acp.url,
            pin: Pin::Integrity(&pinned.codex_acp.integrity),
            dest: codex_tgz.clone(),
        },
        Download {
            url: &pinned.esbuild_wasm.url,
            pin: Pin::Integrity(&pinned.esbuild_wasm.integrity),
            dest: esbuild_tgz.clone(),
        },
    ])?;

    let node = contents.join("Helpers").join("node");
    let members = [
        ("bin/node", node.clone()),
        ("LICENSE", runtime.join("node-LICENSE")),
    ];
    unpack::extract_members(&node_tgz, &members)?;
    let entitlements = source.join("node.entitlements.plist");
    let cdhash = sign::sign(&node, &pinned.node.identifier, &entitlements)?;
    let codex = runtime.join(format!("codex-acp-{}", pinned.codex_acp.version));
    let members = [
        ("dist/index.js", codex.join("index.mjs")),
        ("LICENSE", codex.join("LICENSE")),
    ];
    unpack::extract_members(&codex_tgz, &members)?;
    let esbuild = runtime.join(format!("esbuild-wasm-{}", pinned.esbuild_wasm.version));
    unpack::extract(&esbuild_tgz, &esbuild)?;
    let lockfile_path = &pinned.claude_agent_acp.lockfile;
    let lockfile =
        fs::read_to_string(source.join(lockfile_path)).context("cannot read the lockfile")?;
    unpack::write_file(&runtime.join(lockfile_path), lockfile.as_bytes(), false)?;
    let script = fs::read(source.join(SCRIPT)).context("cannot read the bundle script")?;
    unpack::write_file(&runtime.join(SCRIPT), &script, false)?;

    eprintln!("bundling claude-agent-acp in an isolated directory");
    // The repository's copy of the script: same bytes, and a directory with no download in it.
    let tools = claude::Tools {
        node: &node,
        script: &source.join(SCRIPT),
        esbuild_main: &esbuild.join("lib").join("main.js"),
    };
    let isolated = cache.join("claude-agent-acp.isolated");
    let bundle = claude::bundle(&isolated, &lockfile, &pinned.claude_agent_acp.entry, &tools)?;

    let mut computed = pinned.clone();
    computed.claude_agent_acp.bundle_sha256 = bundle;
    computed.files = hash_tree(&contents)?;
    let node_hash = computed
        .files
        .get_mut("Helpers/node")
        .context("Node is not staged")?;
    node_hash.cdhash = Some(cdhash);

    let licenses = [
        (runtime.join("node-LICENSE"), "node-LICENSE"),
        (codex.join("LICENSE"), "codex-acp-LICENSE"),
        (esbuild.join("LICENSE.md"), "esbuild-wasm-LICENSE.md"),
    ];
    conclude(&source, &pinned, &computed, &licenses, verify)?;
    eprintln!("runtime staged in target/runtime-pack/Contents/");
    Ok(())
}

/// Either checks the manifest and the license texts in `source` (`verify`)
/// or writes them.
fn conclude(
    source: &Path,
    pinned: &Manifest,
    computed: &Manifest,
    licenses: &[(PathBuf, &str)],
    verify: bool,
) -> Result<()> {
    let mut found = if verify {
        manifest::differences(pinned, computed)
    } else {
        Vec::new()
    };
    for (staged, name) in licenses {
        let text = fs::read(staged).with_context(|| format!("cannot read {}", staged.display()))?;
        let path = source.join("licenses").join(name);
        if !verify {
            unpack::write_file(&path, &text, false)?;
        } else if fs::read(&path).ok() != Some(text) {
            found.push(format!(
                "runtime/licenses/{name} differs from the component's"
            ));
        }
    }
    if !verify {
        let path = source.join("manifest.json");
        fs::write(path, computed.to_text()).context("cannot write the manifest")?;
        eprintln!("wrote runtime/manifest.json and runtime/licenses/");
        return Ok(());
    }
    if !found.is_empty() {
        bail!(
            "the runtime does not match runtime/manifest.json:\n  {}",
            found.join("\n  ")
        );
    }
    eprintln!("the runtime matches runtime/manifest.json");
    Ok(())
}

/// The sha-256 of every file below `dir`, keyed by its `/`-separated path.
fn hash_tree(dir: &Path) -> Result<BTreeMap<String, FileHash>> {
    let mut files = BTreeMap::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in
            fs::read_dir(&next).with_context(|| format!("cannot list {}", next.display()))?
        {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            let relative = path.strip_prefix(dir).context("outside the stage")?;
            let parts: Vec<_> = relative.iter().map(|part| part.to_string_lossy()).collect();
            let sha256 = fetch::sha256_hex(&fs::read(&path)?);
            files.insert(
                parts.join("/"),
                FileHash {
                    sha256,
                    cdhash: None,
                },
            );
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn manifest() -> Manifest {
        let text = r#"{
          "schema": 1,
          "node": { "version": "1", "url": "u", "sha256": "s", "identifier": "i" },
          "codex_acp": { "version": "1", "url": "u", "integrity": "i" },
          "esbuild_wasm": { "version": "1", "url": "u", "integrity": "i" },
          "claude_agent_acp": { "version": "1", "lockfile": "l", "entry": "e", "bundle_sha256": "b" },
          "files": { "Helpers/node": { "sha256": "n", "cdhash": "c" } }
        }"#;
        serde_json::from_str(text).expect("valid manifest")
    }

    #[test]
    fn the_committed_manifest_is_canonical() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtime/manifest.json");
        let committed = Manifest::load(&path).expect("runtime/manifest.json parses");
        let text = fs::read_to_string(&path).expect("read");
        assert_eq!(committed.to_text(), text, "written by `runtime-pack`");
    }

    #[test]
    fn verify_reports_every_difference_and_packing_removes_them() {
        let dir = TempDir::new("pack-verify");
        let staged = dir.path().join("LICENSE");
        fs::write(&staged, "license").expect("write");
        let licenses = [(staged, "node-LICENSE")];
        let pinned = manifest();
        let mut computed = manifest();
        computed.claude_agent_acp.bundle_sha256 = "other".to_owned();
        computed.files.remove("Helpers/node");

        let err = conclude(dir.path(), &pinned, &computed, &licenses, true).expect_err("differs");
        let text = format!("{err:#}");
        for expected in [
            "/claude_agent_acp/bundle_sha256: \"b\" -> \"other\"",
            "/files/Helpers/node: {",
            "runtime/licenses/node-LICENSE differs",
        ] {
            assert!(text.contains(expected), "{expected} in {text}");
        }

        conclude(dir.path(), &pinned, &computed, &licenses, false).expect("pack");
        let written = Manifest::load(&dir.path().join("manifest.json")).expect("written");
        assert_eq!(written, computed);
        conclude(dir.path(), &written, &computed, &licenses, true).expect("now it matches");
    }
}
