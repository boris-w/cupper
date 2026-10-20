//! The runtime directory on disk (ADR-08). `<component>/<version>/` is one
//! committed version, never changed after the `rename(2)` that commits it;
//! `.staging-<version>` is one being filled and `.discard-<version>` a broken
//! one on its way out. A run killed at any point leaves only those two, and
//! the next run removes them.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use super::plan::ComponentPlan;
use super::{Bundle, Component, Reason};
use crate::manifest::FileHash;

/// Written last into a staging directory: a version directory without it
/// was never committed.
const INSTALLED: &str = ".installed.json";

/// What a version directory holds, compared with its plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Intact,
    Absent,
    /// Not committed, changed since, or committed from another manifest.
    Broken,
}

impl Verdict {
    pub fn name(self) -> &'static str {
        match self {
            Self::Intact => "ready",
            Self::Absent => "missing",
            Self::Broken => "broken",
        }
    }
}

/// `.installed.json`.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    component: String,
    version: String,
    files: BTreeMap<String, FileHash>,
}

impl Record {
    fn of(plan: &ComponentPlan) -> Self {
        let files = plan
            .files
            .iter()
            .map(|(path, f)| (path.clone(), f.hash.clone()));
        Self {
            component: plan.component.name().to_owned(),
            version: plan.version.clone(),
            files: files.collect(),
        }
    }
}

/// One component's directory in the runtime directory.
pub(crate) struct Slot<'a> {
    dir: PathBuf,
    plan: &'a ComponentPlan,
}

impl<'a> Slot<'a> {
    pub fn new(runtime_dir: &Path, plan: &'a ComponentPlan) -> Self {
        Self {
            dir: runtime_dir.join(plan.component.name()),
            plan,
        }
    }

    fn version_dir(&self) -> PathBuf {
        self.dir.join(&self.plan.version)
    }

    fn aside(&self, kind: &str) -> PathBuf {
        self.dir.join(format!(".{kind}-{}", self.plan.version))
    }

    /// Removes the staging and discarded directories a killed run left.
    pub fn clean(&self) -> io::Result<()> {
        for entry in entries(&self.dir)? {
            let name = entry.file_name();
            let name = name.as_encoded_bytes();
            if name.starts_with(b".staging-") || name.starts_with(b".discard-") {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }

    /// Every file of the plan is there with its hash, under the record of
    /// this plan.
    pub fn check(&self) -> Verdict {
        let dir = self.version_dir();
        match fs::symlink_metadata(&dir) {
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Verdict::Absent,
            Ok(meta) if meta.is_dir() => {}
            _ => return Verdict::Broken,
        }
        let record = fs::read(dir.join(INSTALLED)).ok();
        let record = record.and_then(|text| serde_json::from_slice::<Record>(&text).ok());
        let mut files = self.plan.files.iter();
        match record == Some(Record::of(self.plan))
            && files.all(|(path, f)| verify(&dir.join(path), &f.hash).is_ok())
        {
            true => Verdict::Intact,
            false => Verdict::Broken,
        }
    }

    /// Copies the plan's files into a staging directory and commits it.
    pub fn install(&self, bundle: &dyn Bundle, replace: bool) -> Result<(), Reason> {
        let staging = self.aside("staging");
        let result = self.fill(bundle, &staging).and_then(|()| {
            if replace {
                fs::rename(self.version_dir(), self.aside("discard")).map_err(disk)?;
            }
            fs::rename(&staging, self.version_dir()).map_err(disk)
        });
        // What cannot be removed now, the next run's `clean` removes.
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_dir_all(self.aside("discard"));
        result
    }

    fn fill(&self, bundle: &dyn Bundle, staging: &Path) -> Result<(), Reason> {
        let _ = fs::remove_dir_all(staging);
        let mut dirs = DirBuilder::new();
        dirs.recursive(true).mode(0o700);
        dirs.create(staging).map_err(disk)?;
        for (path, file) in &self.plan.files {
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                dirs.create(parent).map_err(disk)?;
            }
            copy(bundle, &file.source, &target, &file.hash)?;
        }
        let record = serde_json::to_vec(&Record::of(self.plan)).expect("a record serializes");
        fs::write(staging.join(INSTALLED), record).map_err(disk)
    }

    /// Removes the component's other versions, except those in `in_use`.
    pub fn recycle(&self, in_use: &BTreeSet<(Component, String)>) -> io::Result<()> {
        for entry in entries(&self.dir)? {
            let name = entry.file_name();
            let Some(version) = name.to_str() else {
                continue;
            };
            let used = in_use.contains(&(self.plan.component, version.to_owned()));
            if entry.file_type()?.is_dir()
                && !version.starts_with('.')
                && version != self.plan.version
                && !used
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
        Ok(())
    }
}

fn disk(_: io::Error) -> Reason {
    Reason::Disk
}

/// The entries of `dir`; none before it exists.
fn entries(dir: &Path) -> io::Result<Vec<fs::DirEntry>> {
    match fs::read_dir(dir) {
        Ok(entries) => entries.collect(),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(err) => Err(err),
    }
}

/// Copies one bundled file, hashing the bytes as it writes them: a bundle
/// changed or replaced during the copy gives a hash that does not match.
fn copy(bundle: &dyn Bundle, source: &str, target: &Path, hash: &FileHash) -> Result<(), Reason> {
    let from = bundle.open(source).map_err(|_| Reason::BundleChanged)?;
    let meta = from.metadata().map_err(disk)?;
    if !meta.is_file() {
        return Err(Reason::BundleChanged);
    }
    let mode = if meta.permissions().mode() & 0o111 != 0 {
        0o500
    } else {
        0o400
    };
    let mut options = OpenOptions::new();
    let to = options.write(true).create_new(true).mode(mode).open(target);
    let to = to.map_err(disk)?;
    let sha256 = hash_copy(from, to).map_err(disk)?;
    if sha256 != hash.sha256 || !cdhash_matches(target, hash) {
        return Err(Reason::BundleChanged);
    }
    Ok(())
}

/// Checks the file at `path` against `hash`. The error names what differs:
/// `missing`, `unreadable`, `sha256` or `cdhash`.
pub(crate) fn verify(path: &Path, hash: &FileHash) -> Result<(), &'static str> {
    let file = File::open(path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => "missing",
        _ => "unreadable",
    })?;
    if !file.metadata().is_ok_and(|meta| meta.is_file()) {
        return Err("unreadable");
    }
    let sha256 = hash_copy(file, io::sink()).map_err(|_| "unreadable")?;
    if sha256 != hash.sha256 {
        return Err("sha256");
    }
    if !cdhash_matches(path, hash) {
        return Err("cdhash");
    }
    Ok(())
}

/// A Mach-O's signature gives the cdhash the manifest has for it.
fn cdhash_matches(path: &Path, hash: &FileHash) -> bool {
    hash.cdhash
        .as_ref()
        .is_none_or(|want| cupper_sys::file_cdhash(path).is_ok_and(|got| got.to_string() == *want))
}

/// Reads `from` to its end, writing each chunk to `to` once it is hashed;
/// the lowercase hex sha-256 of what was written.
fn hash_copy(mut from: File, mut to: impl Write) -> io::Result<String> {
    let mut sha = Sha256::new();
    let mut buf = vec![0; 1 << 16];
    loop {
        let n = match from.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        sha.update(&buf[..n]);
        to.write_all(&buf[..n])?;
    }
    Ok(format!("{:x}", sha.finalize()))
}
