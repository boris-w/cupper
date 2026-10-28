//! The runtime installer (ADR-08, design 5.2.12). While the host runs it is
//! the one reader of the app bundle: it copies the bundled components into
//! `<data dir>/runtime/<component>/<version>/`, hashing every file against
//! the manifest compiled into the host as it copies, downloads
//! claude-agent-acp's npm tree and bundles it there with them, and commits
//! each version with one `rename(2)`.
//!
//! The pure core in `machine` decides; [`Installer`] carries out its actions
//! and publishes each component's [`Status`]. codex-acp joins in T111.

#[cfg(test)]
mod download_tests;
mod files;
mod machine;
pub mod npm;
mod plan;
pub mod self_test;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use cupper_sys::{Command, Env};
use files::{Slot, copy_in};
pub use files::{Verdict, verify};
use machine::{Action, Input, InstallCore};
pub use plan::{ADAPTER, ComponentPlan, Npm, Source, plans};

use cupper_protocol::index::{RuntimeFailure, RuntimeState};

use crate::clock::Clock;
use crate::driver::probe;
use crate::durations::{self, Durations};
use crate::manifest::FileHash;
use crate::pure::{Core, TimerTable};
use crate::quiescence::ActorKind;

/// The installer's kind in the quiescence barrier: busy between its waits,
/// which are downloads, bundling and file work as well as its timers.
pub const INSTALLER: ActorKind = ActorKind::new("runtime_installer");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Component {
    Node,
    EsbuildWasm,
    ClaudeAgentAcp,
}

impl Component {
    /// Its directory in the runtime directory.
    pub fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::EsbuildWasm => "esbuild-wasm",
            Self::ClaudeAgentAcp => "claude-agent-acp",
        }
    }

    /// What must be ready before it is installed.
    pub fn deps(self) -> &'static [Component] {
        match self {
            Self::ClaudeAgentAcp => &[Self::Node, Self::EsbuildWasm],
            _ => &[],
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
    /// The registry could not be reached, failed or took too long; retried.
    Network,
    /// A download is not what the lockfile pins, or cannot be accepted: a
    /// tarball path that leaves its package; never retried.
    HashMismatch,
}

pub type Statuses = BTreeMap<Component, Status>;

/// Claude's adapter runtime as `agents.updated` shows it: the first failure,
/// else installing, else ready once every component is.
pub fn adapter_state(statuses: &Statuses) -> (RuntimeState, Option<RuntimeFailure>) {
    let failed = statuses.values().find_map(|status| match status {
        Status::Failed(reason) => Some(*reason),
        _ => None,
    });
    let failure = failed.map(|reason| match reason {
        Reason::BundleChanged => RuntimeFailure::BundleChanged,
        Reason::Disk => RuntimeFailure::Disk,
        Reason::Network => RuntimeFailure::Network,
        Reason::HashMismatch => RuntimeFailure::HashMismatch,
    });
    let any = |wanted: Status| statuses.values().any(|status| *status == wanted);
    let state = match () {
        () if failure.is_some() => RuntimeState::Failed,
        () if any(Status::Installing) => RuntimeState::Installing,
        () if statuses.values().all(|status| *status == Status::Ready) => RuntimeState::Ready,
        () => RuntimeState::Missing,
    };
    (state, failure)
}

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

    /// Whether the host runs from an app bundle; outside one, as in
    /// development and tests, there is no runtime to install.
    pub fn is_app(&self) -> bool {
        self.0.ends_with("Contents")
    }
}

impl Bundle for BundleDir {
    fn open(&self, path: &str) -> io::Result<File> {
        File::open(self.0.join(path))
    }
}

/// A file an adapter starts from, with the hash it must have then (ADR-08).
#[derive(Debug, Clone)]
pub struct Installed {
    pub path: PathBuf,
    pub hash: FileHash,
}

/// What Claude's adapter starts from in `runtime_dir`: the node and bundle
/// that the installer installs by `plans`.
pub fn claude_adapter(runtime_dir: &Path, plans: &[ComponentPlan]) -> (Installed, Installed) {
    let file = |component: Component, name: &str| {
        let plan = plans.iter().find(|p| p.component == component);
        let plan = plan.expect("the manifest plans every component");
        let path = runtime_dir.join(component.name()).join(&plan.version);
        let (path, hash) = (path.join(name), plan.files[name].clone());
        Installed { path, hash }
    };
    let node = file(Component::Node, "node");
    (node, file(Component::ClaudeAgentAcp, ADAPTER))
}

/// Installs the components of `plans` into the runtime directory.
pub struct Installer {
    pub plans: Vec<ComponentPlan>,
    pub bundle: Arc<dyn Bundle>,
    /// `<data dir>/runtime`.
    pub dir: PathBuf,
    /// The versions running adapters were started from: once the current
    /// version is ready, the others are removed, except these.
    pub in_use: BTreeSet<(Component, String)>,
    /// [`npm::REGISTRY`], or `test_config.runtime.registry_url`.
    pub registry: String,
    /// The login shell's `HTTPS_PROXY`.
    pub proxy: Option<String>,
    pub durations: Durations,
}

impl Installer {
    /// Checks and installs every component, calling `publish` whenever a
    /// status changes; it waits on `clock`.
    pub async fn run(&self, clock: &dyn Clock, mut publish: impl FnMut(&Statuses)) {
        let retry = self.durations.get(durations::RUNTIME_RETRY);
        let mut core = InstallCore::new(self.plans.iter().map(|p| p.component), retry);
        let mut timers = TimerTable::new();
        let mut actions = VecDeque::from(core.step(Input::Start, clock.now()));
        loop {
            let input = if let Some(action) = actions.pop_front() {
                if let Some(request) = InstallCore::timer_request(&action) {
                    timers.apply(request);
                    continue;
                }
                match self.execute(clock, action).await {
                    Some(input) => input,
                    None => continue,
                }
            } else if let Some(at) = timers.next_deadline() {
                clock.timer_at(at).await;
                match timers.pop_due(clock.now_mono()) {
                    Some(key) => InstallCore::timer_expired(key),
                    None => continue,
                }
            } else {
                return;
            };
            let before = core.statuses().clone();
            actions.extend(core.step(input, clock.now()));
            if *core.statuses() != before {
                publish(core.statuses());
            }
        }
    }

    fn slot(&self, component: Component) -> Slot {
        let plan = self.plans.iter().find(|p| p.component == component);
        Slot::new(&self.dir, plan.expect("a planned component").clone())
    }

    async fn execute(&self, clock: &dyn Clock, action: Action) -> Option<Input> {
        match action {
            Action::Check(c) => {
                let slot = self.slot(c);
                let checked = blocking(move || match slot.clean() {
                    Ok(()) => Input::Checked(c, slot.check()),
                    Err(_) => Input::Installed(c, Err(Reason::Disk)),
                });
                Some(checked.await)
            }
            Action::Install { component, replace } => {
                let result = self.install(clock, self.slot(component), replace).await;
                let name = component.name();
                tracing::info!(component = name, "runtime install: {result:?}");
                Some(Input::Installed(component, result))
            }
            Action::Recycle(c) => {
                let (slot, in_use) = (self.slot(c), self.in_use.clone());
                if let Err(err) = blocking(move || slot.recycle(&in_use)).await {
                    tracing::warn!(component = c.name(), "old versions stay: {err}");
                }
                None
            }
            Action::Timer(_) => unreachable!("`run` keeps the timers"),
        }
    }

    async fn install(&self, clock: &dyn Clock, slot: Slot, replace: bool) -> Result<(), Reason> {
        let bundle = Arc::clone(&self.bundle);
        let npm = match slot.plan.source.clone() {
            Source::Bundle(sources) => {
                return blocking(move || slot.install(bundle.as_ref(), &sources, replace)).await;
            }
            Source::Npm(npm) => npm,
        };
        let s = slot.clone();
        let staging = blocking(move || {
            let staging = s.begin()?;
            let inputs = [
                ("package-lock.json", &npm.lockfile.0, &npm.lockfile.1),
                ("bundle.mjs", &npm.script.0, &npm.script.1),
            ];
            let inputs = inputs.map(|(path, source, hash)| (path, source.as_str(), hash));
            copy_in(bundle.as_ref(), &staging.join("work"), inputs)
                .map(|()| staging)
                .map_err(|reason| s.abandon(reason))
        });
        let staging = staging.await?;
        let built = self.build(clock, &staging.join("work"), &npm.entry).await;
        blocking(move || {
            let work = staging.join("work");
            let made = built.and_then(|()| {
                fs::rename(work.join(ADAPTER), staging.join(ADAPTER)).map_err(|_| Reason::Disk)
            });
            // The npm tree holds Anthropic's SDK: it never stays (ADR-11).
            let removed = fs::remove_dir_all(&work).map_err(|_| Reason::Disk);
            let made = made.and(removed).and_then(|()| {
                let mut files = slot.plan.files.iter();
                let intact = files.all(|(path, hash)| verify(&staging.join(path), hash).is_ok());
                intact.then_some(()).ok_or(Reason::HashMismatch)
            });
            match made {
                Ok(()) => slot.commit(&staging, replace),
                Err(reason) => Err(slot.abandon(reason)),
            }
        })
        .await
    }

    /// Downloads the npm tree of `work/package-lock.json` into `work` and
    /// bundles it into `work/adapter.mjs` with the installed node and
    /// esbuild-wasm. The script runs with `PATH`, node's directory, as its
    /// only variable: esbuild-wasm starts `node` from it.
    async fn build(&self, clock: &dyn Clock, work: &Path, entry: &str) -> Result<(), Reason> {
        let lockfile = fs::read(work.join("package-lock.json")).map_err(|_| Reason::Disk)?;
        let packages = npm::packages(&lockfile, &self.registry)?;
        let client = npm::client(self.proxy.as_deref())?;
        let limit = self.durations.get(durations::RUNTIME_DOWNLOAD);
        npm::fetch(&client, clock, limit, &packages, work).await?;
        let node = self.slot(Component::Node).version_dir();
        let esbuild = self.slot(Component::EsbuildWasm).version_dir();
        let mut env = Env::new();
        env.set("PATH", node.as_os_str())
            .map_err(|_| Reason::Disk)?;
        let mut command = Command::new(node.join("node"));
        command
            .arg(work.join("bundle.mjs"))
            .arg(esbuild.join("lib/main.js"));
        command
            .arg(work)
            .arg(entry)
            .arg(ADAPTER)
            .env(env)
            .current_dir(work);
        let limit = self.durations.get(durations::RUNTIME_BUNDLE);
        match probe::run::run(clock, command, limit).await {
            Ok(output) if output.status.success() => Ok(()),
            // No bundle of the pinned inputs.
            _ => Err(Reason::HashMismatch),
        }
    }
}

/// File work, off the async workers.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    let joined = tokio::task::spawn_blocking(work).await;
    joined.expect("the installer's file work does not panic")
}
