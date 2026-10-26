//! The CI jobs of design 5.1.6, one file per implemented job (plan 1.5).
//! A job registered with `run: None` is implemented by a later task, which adds
//! its file and fills in `run`; the composites already list it.

mod differential;
mod e2e;
mod license;
mod protocol;
mod rust;
mod security;
pub mod testdata_scan;
mod ts;
mod ui;

use anyhow::Result;

use super::ctx::JobCtx;

pub struct Job {
    pub name: &'static str,
    /// Holds the machine-wide lock while running (plan 1.8).
    pub machine_lock: bool,
    pub run: Option<fn(&mut JobCtx) -> Result<()>>,
}

const fn job(name: &'static str, run: Option<fn(&mut JobCtx) -> Result<()>>) -> Job {
    Job {
        name,
        machine_lock: false,
        run,
    }
}

const fn locked(name: &'static str, run: Option<fn(&mut JobCtx) -> Result<()>>) -> Job {
    Job {
        name,
        machine_lock: true,
        run,
    }
}

pub const JOBS: &[Job] = &[
    job("rust", Some(rust::run)),
    job("protocol", Some(protocol::run)),
    job("ts", Some(ts::run)),
    locked("ui", Some(ui::run)),
    job("license", Some(license::run)),
    job("testdata-scan", Some(testdata_scan::run)),
    job("differential", Some(differential::run)),
    locked("e2e", Some(e2e::run)),
    locked("perf", None),
    job("security-macos14", Some(security::run)),
    job("compat-framework", None),
    job("release-config", None),
    job("codex-drift", None),
    job("release", None),
];

/// Composite targets: every job a PR needs, and the nightly run (plan 1.8).
pub const COMPOSITES: &[(&str, &[&str])] = &[
    (
        "pr",
        &[
            "rust",
            "protocol",
            "ts",
            "license",
            "testdata-scan",
            "ui",
            "e2e",
        ],
    ),
    (
        "nightly",
        &[
            "rust",
            "protocol",
            "ts",
            "license",
            "testdata-scan",
            "differential",
            "ui",
            "e2e",
            "perf",
            "release-config",
            "security-macos14",
            "compat-framework",
            "codex-drift",
        ],
    ),
];

/// The jobs a target runs, or `None` for an unknown name.
pub fn resolve(target: &str) -> Option<Vec<&'static Job>> {
    if let Some((_, members)) = COMPOSITES.iter().find(|(name, _)| *name == target) {
        return Some(members.iter().filter_map(|name| find(name)).collect());
    }
    find(target).map(|job| vec![job])
}

fn find(name: &str) -> Option<&'static Job> {
    JOBS.iter().find(|job| job.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_ci_lock_contention_sensitive_jobs_share_the_machine_lock() {
        let locked: Vec<_> = JOBS
            .iter()
            .filter(|job| job.machine_lock)
            .map(|job| job.name)
            .collect();
        assert_eq!(locked, ["ui", "e2e", "perf"]);
    }

    #[test]
    fn composites_name_registered_jobs_only() {
        for (composite, members) in COMPOSITES {
            assert!(find(composite).is_none(), "`{composite}` is also a job");
            for member in *members {
                assert!(find(member).is_some(), "`{composite}` lists `{member}`");
            }
        }
        let mut names: Vec<_> = JOBS.iter().map(|job| job.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), JOBS.len(), "duplicate job names");
    }
}
