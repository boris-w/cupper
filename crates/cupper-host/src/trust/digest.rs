//! The trust digest and the per-file manifest (design 5.2.6, rulings before
//! T035); the encoding is in the module documentation of `trust`.

use std::collections::BTreeMap;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;

use cupper_protocol::annotations::WirePath;
use cupper_protocol::ids::Digest;
use cupper_protocol::item::DiffChange;
use cupper_protocol::methods::project::TrustedFileChange;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::ScannedFile;

const TAG: &[u8] = b"cupper-trust-v1";

pub(super) fn file_hash(path: &str, target: &Path, content: &[u8]) -> [u8; 32] {
    let mut sha = Sha256::new();
    for field in [path.as_bytes(), target.as_os_str().as_bytes(), content] {
        sha.update((field.len() as u64).to_be_bytes());
        sha.update(field);
    }
    sha.finalize().into()
}

/// The digest of `files`, which are in order of their paths.
pub(super) fn digest(files: &[ScannedFile]) -> Digest {
    let mut sha = Sha256::new();
    sha.update(TAG);
    for file in files {
        sha.update(file.hash);
    }
    Digest::from_sha256(&sha.finalize().into())
}

/// Each file's hash by relative path: `projects.trust_manifest`, a JSON
/// object whose keys are in order (rulings before T035).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Manifest(BTreeMap<String, String>);

impl Manifest {
    pub(super) fn of(files: &[ScannedFile]) -> Self {
        let hex = |hash: &[u8; 32]| hash.iter().map(|b| format!("{b:02x}")).collect();
        Self(
            files
                .iter()
                .map(|file| (file.path.clone(), hex(&file.hash)))
                .collect(),
        )
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a map of strings serializes")
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }

    /// What changed from this manifest to `current`, in order of the paths
    /// (`project/trust_info.diff_from_trusted`).
    pub fn diff(&self, current: &Self) -> Vec<TrustedFileChange> {
        let mut paths: Vec<&String> = self.0.keys().chain(current.0.keys()).collect();
        paths.sort_unstable();
        paths.dedup();
        paths
            .into_iter()
            .filter_map(|path| {
                let change = match (self.0.get(path), current.0.get(path)) {
                    (Some(was), Some(now)) if was == now => return None,
                    (Some(_), Some(_)) => DiffChange::Update,
                    (Some(_), None) => DiffChange::Delete,
                    (None, _) => DiffChange::Add,
                };
                let path = WirePath(path.clone());
                Some(TrustedFileChange { path, change })
            })
            .collect()
    }
}
