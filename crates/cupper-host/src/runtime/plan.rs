//! What each component's version directory holds, and where it comes from,
//! by the manifest.

use std::collections::BTreeMap;

use super::Component;
use crate::manifest::{self, FileHash, Manifest};

/// One version of a component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentPlan {
    pub component: Component,
    pub version: String,
    /// What the version directory holds, by path, with the hash of each.
    pub files: BTreeMap<String, FileHash>,
    pub source: Source,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Copied from the bundle: each file's path under
    /// `CupperHost.app/Contents`, by its path in the version directory.
    Bundle(BTreeMap<String, String>),
    /// Downloaded by a lockfile and bundled here into [`ADAPTER`].
    Npm(Npm),
}

/// The inputs of a bundle made on this machine, read from the app bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Npm {
    pub lockfile: (String, FileHash),
    pub script: (String, FileHash),
    /// The entry point in the npm tree.
    pub entry: String,
}

/// claude-agent-acp's bundle in its version directory.
pub const ADAPTER: &str = "adapter.mjs";
/// The script that makes it, in the app bundle.
pub const SCRIPT: &str = "Resources/runtime/bundle-claude-acp.mjs";

/// The components this host installs. codex-acp is not among them (T111).
pub fn plans(manifest: &Manifest) -> Vec<ComponentPlan> {
    let (node, esbuild) = (&manifest.node.version, &manifest.esbuild_wasm.version);
    let mut plans = split(node, esbuild, &manifest.files);
    let claude = &manifest.claude_agent_acp;
    let lockfile = format!("Resources/runtime/{}", claude.lockfile);
    let hash = &claude.bundle_sha256;
    plans.push(npm(
        &claude.version,
        &manifest.files,
        &lockfile,
        &claude.entry,
        hash,
    ));
    plans
}

/// Node is `Helpers/node` and its licence; esbuild-wasm is the package
/// directory `Resources/runtime/esbuild-wasm-<version>/`.
pub(crate) fn split(
    node: &str,
    esbuild: &str,
    files: &BTreeMap<String, FileHash>,
) -> Vec<ComponentPlan> {
    let empty = |component, version: &str| ComponentPlan {
        component,
        version: version.to_owned(),
        files: BTreeMap::new(),
        source: Source::Bundle(BTreeMap::new()),
    };
    let mut plans = [
        empty(Component::Node, node),
        empty(Component::EsbuildWasm, esbuild),
    ];
    let package = format!("Resources/runtime/esbuild-wasm-{esbuild}/");
    for (source, hash) in files {
        let (plan, target) = match source.as_str() {
            manifest::NODE => (&mut plans[0], "node"),
            "Resources/runtime/node-LICENSE" => (&mut plans[0], "LICENSE"),
            other => match other.strip_prefix(&package) {
                Some(target) => (&mut plans[1], target),
                None => continue,
            },
        };
        plan.files.insert(target.to_owned(), hash.clone());
        if let Source::Bundle(sources) = &mut plan.source {
            sources.insert(target.to_owned(), source.clone());
        }
    }
    plans.into()
}

/// claude-agent-acp: [`ADAPTER`], made from the npm tree of `lockfile` by
/// [`SCRIPT`], both in `files`.
pub(crate) fn npm(
    version: &str,
    files: &BTreeMap<String, FileHash>,
    lockfile: &str,
    entry: &str,
    bundle_sha256: &str,
) -> ComponentPlan {
    let input = |path: &str| {
        let hash = files
            .get(path)
            .expect("the manifest lists the bundling inputs");
        (path.to_owned(), hash.clone())
    };
    let bundle = FileHash {
        sha256: bundle_sha256.to_owned(),
        cdhash: None,
    };
    ComponentPlan {
        component: Component::ClaudeAgentAcp,
        version: version.to_owned(),
        files: BTreeMap::from([(ADAPTER.to_owned(), bundle)]),
        source: Source::Npm(Npm {
            lockfile: input(lockfile),
            script: input(SCRIPT),
            entry: entry.to_owned(),
        }),
    }
}
