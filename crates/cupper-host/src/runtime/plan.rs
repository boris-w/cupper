//! What each component's version directory holds, from the manifest.

use std::collections::BTreeMap;

use super::Component;
use crate::manifest::{FileHash, Manifest};

/// One version of a component: its files by their path in the version
/// directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentPlan {
    pub component: Component,
    pub version: String,
    pub files: BTreeMap<String, BundledFile>,
}

/// A file copied from the bundle, with the hash it must have.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledFile {
    /// Its path under `CupperHost.app/Contents`.
    pub source: String,
    pub hash: FileHash,
}

/// The components this host installs from its bundle. The manifest's other
/// files are not copied here: codex-acp (T111), the bundling script and the
/// lockfile (T115).
pub fn plans(manifest: &Manifest) -> Vec<ComponentPlan> {
    split(
        &manifest.node.version,
        &manifest.esbuild_wasm.version,
        &manifest.files,
    )
}

/// Node is `Helpers/node` and its licence; esbuild-wasm is the package
/// directory `Resources/runtime/esbuild-wasm-<version>/`.
pub(crate) fn split(
    node: &str,
    esbuild: &str,
    files: &BTreeMap<String, FileHash>,
) -> Vec<ComponentPlan> {
    let mut plans = [
        ComponentPlan::empty(Component::Node, node),
        ComponentPlan::empty(Component::EsbuildWasm, esbuild),
    ];
    let package = format!("Resources/runtime/esbuild-wasm-{esbuild}/");
    for (source, hash) in files {
        let (plan, target) = match source.as_str() {
            "Helpers/node" => (&mut plans[0], "node"),
            "Resources/runtime/node-LICENSE" => (&mut plans[0], "LICENSE"),
            other => match other.strip_prefix(&package) {
                Some(target) => (&mut plans[1], target),
                None => continue,
            },
        };
        let file = BundledFile {
            source: source.clone(),
            hash: hash.clone(),
        };
        plan.files.insert(target.to_owned(), file);
    }
    plans.into()
}

impl ComponentPlan {
    fn empty(component: Component, version: &str) -> Self {
        Self {
            component,
            version: version.to_owned(),
            files: BTreeMap::new(),
        }
    }
}
