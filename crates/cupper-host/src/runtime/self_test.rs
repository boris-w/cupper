//! `cupper-host --self-test` (design 5.10.7): do the bundled files match the
//! manifest compiled into the host? The release pipeline runs it on the
//! signed bundle, where no runtime is installed yet, so only that decides the
//! exit code; the installed runtime is one more field of the report.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use serde::Serialize;

use super::files::{Slot, verify};
use super::{BundleDir, ComponentPlan, plans, runtime_dir};
use crate::manifest::{self, FileHash};

/// Printed as one line of JSON.
#[derive(Debug, Serialize)]
pub struct Report {
    /// No problems: exit 0.
    pub ok: bool,
    /// How many bundled files the manifest lists.
    pub files: usize,
    pub problems: Vec<Problem>,
    /// Each component's version directory: `ready`, `missing` or `broken`.
    pub runtime: BTreeMap<&'static str, &'static str>,
}

#[derive(Debug, Serialize)]
pub struct Problem {
    pub file: String,
    /// `missing`, `unreadable`, `sha256` or `cdhash`.
    pub problem: &'static str,
}

/// This host's: its embedded manifest and bundle, and the runtime directory
/// under `data_dir`, which is only read.
pub fn for_host(data_dir: &Path) -> io::Result<Report> {
    let manifest = manifest::embedded();
    let bundle = BundleDir::of_this_host()?;
    let plans = plans(manifest);
    Ok(check(
        &manifest.files,
        &bundle,
        &plans,
        &runtime_dir(data_dir),
    ))
}

pub(crate) fn check(
    files: &BTreeMap<String, FileHash>,
    bundle: &BundleDir,
    plans: &[ComponentPlan],
    runtime_dir: &Path,
) -> Report {
    let problems = files.iter().filter_map(|(file, hash)| {
        let problem = verify(&bundle.0.join(file), hash).err()?;
        let file = file.clone();
        Some(Problem { file, problem })
    });
    let problems: Vec<_> = problems.collect();
    let runtime = plans.iter().map(|plan| {
        let verdict = Slot::new(runtime_dir, plan).check();
        (plan.component.name(), verdict.name())
    });
    Report {
        ok: problems.is_empty(),
        files: files.len(),
        problems,
        runtime: runtime.collect(),
    }
}
