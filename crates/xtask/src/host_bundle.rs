//! `cargo xtask host-bundle --kind <kind>` (design 5.10.1, 5.10.2, 5.7.9):
//! builds the host with the kind's features, assembles `CupperHost.app`
//! around it and the runtime that `runtime-pack` staged, and signs it ad hoc;
//! then builds the CLI, a self-use CLI with the host's cdhash compiled in
//! (design 5.8.1), and signs it. The runtime must match `runtime/manifest.json`
//! file by file, and Node keeps the signature `runtime-pack` gave it. Cargo
//! builds into `target/host-bundle/build/`, so the binaries that tests run from
//! `target/debug/` are never replaced; the results go to
//! `target/host-bundle/<kind>/`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::ci::git;
use crate::codesign::{self, Signing};
use crate::runtime_pack::hash_tree;
use crate::runtime_pack::manifest::Manifest;

/// A build kind of design 5.7.9.
pub struct Kind {
    pub name: &'static str,
    pub host_id: &'static str,
    pub host_features: &'static [&'static str],
    pub cli_id: &'static str,
    pub cli_features: &'static [&'static str],
    /// On for release and self-use builds; off for e2e builds (ad hoc with
    /// hardened runtime needs library validation off) and dev builds.
    pub hardened_runtime: bool,
}

pub const KINDS: &[Kind] = &[
    Kind {
        name: "release",
        host_id: "dev.cupper.app.host",
        host_features: &[],
        cli_id: "dev.cupper.cli",
        cli_features: &[],
        hardened_runtime: true,
    },
    Kind {
        name: "self-use",
        host_id: "dev.cupper.app.local.host",
        host_features: &["dev-auth"],
        cli_id: "dev.cupper.cli.local",
        cli_features: &["self-use"],
        hardened_runtime: true,
    },
    Kind {
        name: "e2e",
        host_id: "dev.cupper.app.e2e.host",
        host_features: &["dev-auth", "test-harness"],
        cli_id: "dev.cupper.cli.e2e",
        cli_features: &["test-harness"],
        hardened_runtime: false,
    },
    Kind {
        name: "dev",
        host_id: "dev.cupper.app.dev.host",
        host_features: &["dev-auth"],
        cli_id: "dev.cupper.cli.dev",
        cli_features: &["dev-auth"],
        hardened_runtime: false,
    },
];

pub const USAGE: &str = "usage: cargo xtask host-bundle --kind <release|self-use|e2e|dev> \
    [--runtime <dir>] [--debug]\n\n\
    Builds, assembles and signs target/host-bundle/<kind>/CupperHost.app and cupper.\n\
    --runtime <dir>  the runtime (default: target/runtime-pack/Contents, which\n\
    \x20                `cargo xtask runtime-pack` stages)\n\
    --debug          build with the dev profile instead of release";

pub struct Options {
    pub kind: &'static Kind,
    /// The runtime, laid out like `CupperHost.app/Contents`.
    pub runtime: PathBuf,
    /// The manifest the runtime must match.
    pub manifest: PathBuf,
    /// Where `CupperHost.app` and `cupper` go; replaced on every run.
    pub out: PathBuf,
    pub release: bool,
}

pub struct Bundle {
    pub app: PathBuf,
    pub host_cdhash: String,
    pub cli: PathBuf,
    pub cli_cdhash: String,
}

pub fn main(args: &[String]) -> ExitCode {
    let outcome = std::env::current_dir()
        .context("no current directory")
        .and_then(|cwd| git::toplevel(&cwd))
        .and_then(|root| {
            let options = parse(args, &root)?;
            let bundle = bundle(&root, &options)?;
            let shown = |path: &Path| {
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .display()
                    .to_string()
            };
            println!("{}  cdhash {}", shown(&bundle.app), bundle.host_cdhash);
            println!("{}  cdhash {}", shown(&bundle.cli), bundle.cli_cdhash);
            Ok(())
        });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("xtask host-bundle: {err:#}\n\n{USAGE}");
            ExitCode::FAILURE
        }
    }
}

fn parse(args: &[String], root: &Path) -> Result<Options> {
    let (mut kind, mut runtime, mut release) = (None, None, true);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--kind" => {
                let name = rest.next().context("`--kind` needs a build kind")?;
                let found = KINDS.iter().find(|kind| kind.name == name);
                kind = Some(found.with_context(|| format!("unknown build kind `{name}`"))?);
            }
            "--runtime" => {
                runtime = Some(PathBuf::from(
                    rest.next().context("`--runtime` needs a directory")?,
                ))
            }
            "--debug" => release = false,
            other => bail!("unexpected argument `{other}`"),
        }
    }
    let kind: &'static Kind = kind.context("`--kind` is required")?;
    Ok(Options {
        kind,
        runtime: runtime.unwrap_or_else(|| root.join("target/runtime-pack/Contents")),
        manifest: root.join("runtime/manifest.json"),
        out: root.join("target/host-bundle").join(kind.name),
        release,
    })
}

pub fn bundle(root: &Path, options: &Options) -> Result<Bundle> {
    let kind = options.kind;
    let manifest = Manifest::load(&options.manifest)?;
    let node_cdhash = manifest
        .files
        .get("Helpers/node")
        .and_then(|node| node.cdhash.as_deref())
        .context("the manifest has no cdhash for Helpers/node")?;
    check_node(&options.runtime.join("Helpers/node"), node_cdhash)?;
    let build = |package, features, host_cdhash| {
        cargo_build(root, package, features, options.release, host_cdhash)
    };
    let host = build("cupper-host", kind.host_features, None)?;

    let app = options.out.join("CupperHost.app");
    let contents = app.join("Contents");
    if options.out.exists() {
        fs::remove_dir_all(&options.out).context("cannot clear the output directory")?;
    }
    copy_tree(&options.runtime, &contents)?;
    let copied = hash_tree(&contents)?;
    for (path, file) in &manifest.files {
        if copied.get(path).map(|copy| &copy.sha256) != Some(&file.sha256) {
            bail!("the runtime's `{path}` is missing or differs from the manifest");
        }
    }
    if let Some(path) = copied
        .keys()
        .find(|path| !manifest.files.contains_key(*path))
    {
        bail!("the runtime has `{path}`, which the manifest does not list");
    }
    // ADR-11: no part of Anthropic's SDK, and no Claude Code, in the app.
    if let Some(path) = copied
        .keys()
        .find(|path| path.contains("anthropic") || path.contains("claude-agent-sdk"))
    {
        bail!("`{path}` must never be bundled (ADR-11)");
    }
    fs::create_dir_all(contents.join("MacOS")).context("cannot create MacOS/")?;
    fs::copy(&host, contents.join("MacOS/cupper-host")).context("cannot copy the host")?;
    fs::write(contents.join("Info.plist"), info_plist(kind.host_id))
        .context("cannot write Info.plist")?;

    let signing = |identifier| Signing {
        identifier,
        hardened_runtime: kind.hardened_runtime,
        entitlements: None,
    };
    let host_cdhash = codesign::sign(&app, &signing(kind.host_id))?;
    check_node(&contents.join("Helpers/node"), node_cdhash)?;

    let compiled_in = kind
        .cli_features
        .contains(&"self-use")
        .then_some(host_cdhash.as_str());
    let built = build("cupper-cli", kind.cli_features, compiled_in)?;
    let cli = options.out.join("cupper");
    fs::copy(&built, &cli).context("cannot copy the CLI")?;
    let cli_cdhash = codesign::sign(&cli, &signing(kind.cli_id))?;
    Ok(Bundle {
        app,
        host_cdhash,
        cli,
        cli_cdhash,
    })
}

/// Node must keep the signature `runtime-pack` gave it.
fn check_node(node: &Path, cdhash: &str) -> Result<()> {
    let found = codesign::cdhash(node)?;
    if found != cdhash {
        bail!(
            "{} has cdhash {found}, the manifest says {cdhash}",
            node.display()
        );
    }
    Ok(())
}

/// Builds the binary of `package` (`cupper-host` or `cupper-cli`) and returns
/// the path cargo reports for it. `host_cdhash` is the one a self-use CLI
/// compiles in (T045).
fn cargo_build(
    root: &Path,
    package: &str,
    features: &[&str],
    release: bool,
    host_cdhash: Option<&str>,
) -> Result<PathBuf> {
    // `cupper-cli` builds `cupper`.
    let bin = package.strip_suffix("-cli").unwrap_or(package);
    let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()));
    cargo
        .current_dir(root)
        .args(["build", "--locked", "--quiet"])
        .arg("--message-format=json-render-diagnostics")
        .args(["--package", package, "--bin", bin, "--target-dir"])
        .arg(root.join("target/host-bundle/build"))
        .stdin(Stdio::null())
        .stderr(Stdio::inherit());
    if release {
        cargo.arg("--release");
    }
    if !features.is_empty() {
        cargo.args(["--features", &features.join(",")]);
    }
    match host_cdhash {
        Some(cdhash) => cargo.env("CUPPER_HOST_CDHASH", cdhash),
        None => cargo.env_remove("CUPPER_HOST_CDHASH"),
    };
    let output = cargo
        .output()
        .with_context(|| format!("cannot build {package}"))?;
    if !output.status.success() {
        bail!("building {package} failed ({})", output.status);
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|message| {
            message["reason"] == "compiler-artifact" && message["target"]["name"] == bin
        })
        .find_map(|message| message["executable"].as_str().map(PathBuf::from))
        .with_context(|| format!("cargo reported no executable for {bin}"))
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).with_context(|| format!("cannot create {}", to.display()))?;
    for entry in fs::read_dir(from).with_context(|| format!("cannot list {}", from.display()))? {
        let path = entry?.path();
        let target = to.join(path.file_name().context("no file name")?);
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            fs::copy(&path, &target).with_context(|| format!("cannot copy {}", path.display()))?;
        }
    }
    Ok(())
}

/// The host's Info.plist (design 5.10.1): an agent app with no usage
/// descriptions at all (security #13).
fn info_plist(identifier: &str) -> String {
    let version = env!("CARGO_PKG_VERSION");
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleExecutable</key><string>cupper-host</string>
	<key>CFBundleIdentifier</key><string>{identifier}</string>
	<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
	<key>CFBundleName</key><string>CupperHost</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleShortVersionString</key><string>{version}</string>
	<key>CFBundleVersion</key><string>{version}</string>
	<key>LSMinimumSystemVersion</key><string>14.0</string>
	<key>LSUIElement</key><true/>
</dict>
</plist>
"#
    )
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::testutil::TempDir;

    /// A small runtime: an ad-hoc signed Mach-O as Node and two files, with
    /// the committed manifest's `files` replaced by theirs.
    fn fake_runtime(root: &Path, dir: &Path) -> (PathBuf, PathBuf) {
        let runtime = dir.join("runtime");
        let node = runtime.join("Helpers/node");
        fs::create_dir_all(node.parent().expect("parent")).expect("mkdir");
        fs::copy("/usr/bin/true", &node).expect("copy a Mach-O");
        let signing = Signing {
            identifier: "dev.cupper.runtime.node",
            hardened_runtime: true,
            entitlements: None,
        };
        let cdhash = codesign::sign(&node, &signing).expect("sign the fake Node");
        let scripts = runtime.join("Resources/runtime");
        fs::create_dir_all(scripts.join("locks")).expect("mkdir");
        fs::write(scripts.join("bundle-claude-acp.mjs"), "// fake\n").expect("write");
        fs::write(scripts.join("locks/claude-agent-acp.json"), "{}\n").expect("write");

        let mut manifest = Manifest::load(&root.join("runtime/manifest.json")).expect("manifest");
        manifest.files = hash_tree(&runtime).expect("hash");
        manifest.files.get_mut("Helpers/node").expect("node").cdhash = Some(cdhash);
        let path = dir.join("manifest.json");
        fs::write(&path, manifest.to_text()).expect("write the manifest");
        (runtime, path)
    }

    #[test]
    fn it_host_bundle_layout_of_a_self_use_build() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let dir = TempDir::new("host-bundle");
        let (runtime, manifest) = fake_runtime(&root, dir.path());
        let kind = KINDS
            .iter()
            .find(|kind| kind.name == "self-use")
            .expect("kind");
        let out = dir.path().join("out");
        let options = Options {
            kind,
            runtime,
            manifest,
            out,
            release: false,
        };
        let built = bundle(&root, &options).expect("assembled and signed");

        let contents = built.app.join("Contents");
        let files: BTreeSet<String> = hash_tree(&contents).expect("hash").into_keys().collect();
        let expected = [
            "Helpers/node",
            "Info.plist",
            "MacOS/cupper-host",
            "Resources/runtime/bundle-claude-acp.mjs",
            "Resources/runtime/locks/claude-agent-acp.json",
            "_CodeSignature/CodeResources",
        ];
        assert_eq!(files, expected.map(str::to_owned).into());

        let signed = |path: &Path, wants: &[&str]| {
            let details = codesign::details(path).expect("signed");
            wants.iter().all(|want| details.contains(want))
        };
        let hardened = "flags=0x10002(adhoc,runtime)";
        assert!(signed(
            &built.app,
            &["Identifier=dev.cupper.app.local.host\n", hardened]
        ));
        assert!(signed(
            &built.cli,
            &["Identifier=dev.cupper.cli.local\n", hardened]
        ));
        let node = contents.join("Helpers/node");
        assert!(
            signed(&node, &["Identifier=dev.cupper.runtime.node\n"]),
            "not re-signed"
        );

        let plist = Command::new("plutil")
            .args(["-convert", "json", "-o", "-"])
            .arg(contents.join("Info.plist"))
            .output()
            .expect("plutil");
        let plist: Value = serde_json::from_slice(&plist.stdout).expect("Info.plist as JSON");
        assert_eq!(plist["CFBundleIdentifier"], "dev.cupper.app.local.host");
        assert_eq!(plist["CFBundleExecutable"], "cupper-host");
        assert_eq!(plist["LSUIElement"], true);
        assert_eq!(plist["LSMinimumSystemVersion"], "14.0");
        let keys: Vec<&String> = plist.as_object().expect("a dictionary").keys().collect();
        assert!(
            !keys.iter().any(|key| key.contains("UsageDescription")),
            "{keys:?}"
        );

        let info = Command::new(&built.cli)
            .arg("--build-info")
            .output()
            .expect("cupper");
        let info: Value = serde_json::from_slice(&info.stdout).expect("build info");
        assert_eq!(info["build"], "self_use");
        assert_eq!(info["host_cdhash"], built.host_cdhash.as_str());

        let script = options
            .runtime
            .join("Resources/runtime/bundle-claude-acp.mjs");
        fs::write(script, "// changed\n").expect("write");
        let err = bundle(&root, &options)
            .err()
            .expect("a runtime unlike the manifest");
        assert!(
            err.to_string().contains("differs from the manifest"),
            "{err}"
        );
    }

    #[test]
    fn it_host_bundle_layout_kinds_match_design_5_7_9() {
        let design = include_str!("../../../docs/design/07-desktop.md");
        let row = |label: &str| -> Vec<String> {
            let line = design
                .lines()
                .find(|line| line.starts_with(label))
                .expect(label);
            let cells = line
                .split('|')
                .skip(2)
                .map(|cell| cell.trim().trim_matches('`'));
            cells
                .filter(|cell| !cell.is_empty())
                .map(str::to_owned)
                .collect()
        };
        let hosts: Vec<&str> = KINDS.iter().map(|kind| kind.host_id).collect();
        let clis: Vec<&str> = KINDS.iter().map(|kind| kind.cli_id).collect();
        assert_eq!(row("| 宿主的 bundle id |"), hosts);
        assert_eq!(row("| CLI 的 identifier |"), clis);

        let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        let parsed = parse(&args(&["--kind", "e2e", "--debug"]), Path::new("/r")).expect("valid");
        assert_eq!(parsed.kind.name, "e2e");
        assert_eq!(parsed.runtime, Path::new("/r/target/runtime-pack/Contents"));
        assert_eq!(parsed.out, Path::new("/r/target/host-bundle/e2e"));
        assert!(!parsed.release);
        assert!(parse(&args(&["--kind", "beta"]), Path::new("/r")).is_err());
        assert!(parse(&[], Path::new("/r")).is_err());
    }
}
