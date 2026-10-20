//! The runtime installer (ADR-08, design 5.2.12). While the host runs it is
//! the one reader of the app bundle: it copies the bundled components into
//! `<data dir>/runtime/<component>/<version>/`, hashing every file against
//! the manifest compiled into the host as it copies, and commits each version
//! with one `rename(2)`.
//!
//! The pure core in `machine` decides; [`Installer`] does the file work on
//! the caller's (blocking) thread and publishes each component's [`Status`].
//! claude-agent-acp, downloaded and bundled on this machine, joins in T115,
//! with the `network` and `hash_mismatch` failures; codex-acp in T111.

mod files;
mod machine;
mod plan;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

use files::Slot;
pub use files::Verdict;
use machine::{Action, Input, InstallCore};
pub use plan::{BundledFile, ComponentPlan, plans};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component {
    Node,
    EsbuildWasm,
}

impl Component {
    /// Its directory in the runtime directory.
    pub fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::EsbuildWasm => "esbuild-wasm",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Missing,
    Installing,
    Ready,
    Failed(Reason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// A bundled file is not what the manifest says: the app changed under
    /// the host, which waits for its upgrade (design 5.2.12).
    BundleChanged,
    /// The runtime directory cannot be written.
    Disk,
}

pub type Statuses = BTreeMap<Component, Status>;

/// `<data dir>/runtime`.
pub fn runtime_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("runtime")
}

/// Where the installer reads bundled files.
pub trait Bundle: Send + Sync {
    /// Opens `path`, relative to `CupperHost.app/Contents`.
    fn open(&self, path: &str) -> io::Result<File>;
}

/// A bundle's `Contents` directory.
pub struct BundleDir(pub PathBuf);

impl BundleDir {
    /// The bundle this host runs from, as `Contents/MacOS/cupper-host`.
    pub fn of_this_host() -> io::Result<Self> {
        let exe = std::env::current_exe()?.canonicalize()?;
        let contents = exe.parent().and_then(Path::parent);
        let contents = contents.ok_or_else(|| io::Error::other("the host is not in a bundle"))?;
        Ok(Self(contents.to_owned()))
    }
}

impl Bundle for BundleDir {
    fn open(&self, path: &str) -> io::Result<File> {
        File::open(self.0.join(path))
    }
}

/// Installs the components of `plans` into the runtime directory.
pub struct Installer {
    plans: BTreeMap<Component, ComponentPlan>,
    bundle: Box<dyn Bundle>,
    dir: PathBuf,
    in_use: BTreeSet<(Component, String)>,
}

impl Installer {
    /// `dir` is the runtime directory. Other versions are removed once the
    /// current one is ready, except those in `in_use`: the versions running
    /// adapters were started from.
    pub fn new(
        plans: Vec<ComponentPlan>,
        bundle: Box<dyn Bundle>,
        dir: PathBuf,
        in_use: BTreeSet<(Component, String)>,
    ) -> Self {
        let plans = plans.into_iter().map(|plan| (plan.component, plan));
        Self {
            plans: plans.collect(),
            bundle,
            dir,
            in_use,
        }
    }

    /// Checks and installs every component, calling `publish` whenever a
    /// status changes.
    pub fn run(&self, mut publish: impl FnMut(&Statuses)) {
        let mut core = InstallCore::new(self.plans.keys().copied());
        let mut actions = VecDeque::from(core.step(Input::Start));
        while let Some(action) = actions.pop_front() {
            let Some(input) = self.execute(action) else {
                continue;
            };
            let before = core.statuses().clone();
            actions.extend(core.step(input));
            if *core.statuses() != before {
                publish(core.statuses());
            }
        }
    }

    fn execute(&self, action: Action) -> Option<Input> {
        let component = match action {
            Action::Check(c) | Action::Install { component: c, .. } | Action::Recycle(c) => c,
        };
        let slot = Slot::new(&self.dir, &self.plans[&component]);
        match action {
            Action::Check(_) => Some(match slot.clean() {
                Ok(()) => Input::Checked(component, slot.check()),
                Err(_) => Input::Installed(component, Err(Reason::Disk)),
            }),
            Action::Install { replace, .. } => {
                let result = slot.install(self.bundle.as_ref(), replace);
                let name = component.name();
                tracing::info!(component = name, "runtime install: {result:?}");
                Some(Input::Installed(component, result))
            }
            Action::Recycle(_) => {
                if let Err(err) = slot.recycle(&self.in_use) {
                    tracing::warn!(component = component.name(), "old versions stay: {err}");
                }
                None
            }
        }
    }
}
