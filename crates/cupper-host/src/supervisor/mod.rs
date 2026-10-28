//! The host's lifecycle (design 5.2.13): [`Host::start`] runs startup steps
//! 3 to 10, and [`Host::run`] serves until one of the triggers of the orderly
//! exit comes, then exits.
//!
//! The orderly exit comes on `host/exit` ([`control`]), on SIGTERM, when
//! the store writer has died (T024 review: the host cannot run without it),
//! and from the [`watch`]: idle, orphaned, or once an upgrade waits. Before
//! retention starts, [`reap`] ends the adapters a previous run left.

mod audit;
pub mod control;
#[cfg(feature = "test-harness")]
mod harness;
mod reap;
mod recover;
#[cfg(feature = "test-harness")]
mod seed;
pub mod watch;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::Poll;

use cupper_protocol::domain::AgentKind;
use cupper_protocol::index::HostUpdated;
use cupper_protocol::methods::admin::{Settings, SettingsGet, SettingsUpdate};
use cupper_protocol::methods::handshake::{HostShutdown, HostShutdownParams, ShutdownReason};
use futures::future::BoxFuture;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use self::control::Control;
#[cfg(test)]
pub(crate) use self::recover::end_previous_run;
use self::watch::{Admission, SelfCheck, Watch, WatchDeps};
#[cfg(feature = "test-harness")]
use crate::clock::ManualClock;
use crate::clock::{Clock, IdGen, SystemClock};
use crate::diag::full::Redactor;
use crate::diag::log::log_dir;
use crate::diag::stderr::AdapterLog;
use crate::driver::locate::CliLocator;
use crate::driver::locate::trust::{Boundaries, system_temp_dirs};
use crate::driver::probe::login_shell::LoginEnv;
use crate::driver::runtime::agents::{Agents, Find};
use crate::driver::runtime::launch::{self, Launch};
use crate::driver::runtime::{AGENT_RUNTIME, AgentRuntime, RuntimeDeps};
use crate::durations::Durations;
use crate::exit::HostExit;
use crate::gateway::listen::{HostLock, ListenError};
use crate::gateway::{AuditEvent, AuditRecord, Gateway, GatewayConfig, HostEvent, Port, Routes};
use crate::git::GitTool;
use crate::index::{INDEX, IndexDeps, IndexError, IndexHandle};
use crate::ledger::{Ledger, LedgerError, SWEEPER};
use crate::manifest;
use crate::orchestrator::session::{SessionDeps, SessionManager};
use crate::quiescence::Quiescence;
use crate::requester::ProcessRequesters;
use crate::runtime::{self, BundleDir, ComponentPlan, Installer, Reason, Status, Statuses, npm};
use crate::settings::{SettingsHandle, SettingsPort};
use crate::startup::payload::Payload;
use crate::startup::{DataDirError, prepare_data_dir};
use crate::store::epoch::{self, BootSessionError};
use crate::store::meta::{self, MetaKey};
use crate::store::streams::{Streams, StreamsConfig};
use crate::store::{
    BlobStore, Database, Durability, MIGRATIONS, MigrateError, OpenError, RAW_SCHEMA, RAW_WRITER,
    RawWriter, RawWriterThread, Readers, WRITER, Writer, WriterConfig, WriterThread, migrate,
    open_reader, open_writer, prepare_raw, retention,
};

/// What [`Host::start`] needs from step 1 (`main`).
#[derive(Debug)]
pub struct Start {
    pub data_dir: PathBuf,
    pub payload: Payload,
    /// The unique ID of the process that started the host (design 5.8.1).
    #[cfg(feature = "dev-auth")]
    pub launcher: u64,
}

/// A part of the host with turns, interactions or processes of its own: the
/// sessions (T043), the agent runtimes (T055) and the index (T034) add
/// themselves in [`Host::start`], in that order.
pub trait Part: Send + Sync + fmt::Debug {
    /// What runs in it now, for `host/status` and `host/exit`.
    fn activity(&self) -> Activity;

    /// Its share of the orderly exit, part after part: the sessions end
    /// their running turns as `interrupted` when `force`d and write their
    /// checkpoints, the runtimes close their adapters (ADR-09), the index
    /// writes its checkpoint.
    fn stop(&self, force: bool) -> BoxFuture<'static, ()>;
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Activity {
    pub running_turns: u32,
    pub pending_interactions: u32,
    pub queued_messages: u32,
}

impl std::iter::Sum for Activity {
    fn sum<I: Iterator<Item = Self>>(parts: I) -> Self {
        parts.fold(Self::default(), |sum, part| Self {
            running_turns: sum.running_turns + part.running_turns,
            pending_interactions: sum.pending_interactions + part.pending_interactions,
            queued_messages: sum.queued_messages + part.queued_messages,
        })
    }
}

/// Why the host exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    /// `host/exit`.
    Requested { force: bool },
    /// SIGTERM, e.g. from the desktop restarting a host it cannot reach
    /// (design 5.8.1, Q22): running turns are interrupted.
    Terminated,
    /// The store writer died: exit 1, without `last_clean_exit_at`.
    WriterDied,
    /// No clients and no work for `supervisor_idle`.
    Idle,
    /// The process that started the host exited, and no turn runs.
    Orphaned,
    /// The code on disk changed, and nothing is left to finish.
    Upgrade,
}

/// The running host.
#[derive(Debug)]
pub struct Host {
    clock: Arc<dyn Clock>,
    writer: Writer,
    writer_thread: WriterThread,
    raw_thread: RawWriterThread,
    parts: Arc<[Arc<dyn Part>]>,
    gateway: Arc<Gateway>,
    serving: JoinHandle<()>,
    /// Retention (T027) and the ledger's sweeper (T032).
    sweeps: [JoinHandle<()>; 2],
    /// The audit's way in, and its drain for the exit.
    audit: mpsc::UnboundedSender<AuditRecord>,
    drain: audit::Drain,
    exits: mpsc::UnboundedReceiver<Exit>,
    terminate: Signal,
    socket: PathBuf,
    /// The barrier's registry, for the test that every kind is in it.
    #[cfg(all(test, feature = "test-harness"))]
    quiescence: Quiescence,
    /// Released last, as the host exits.
    _lock: HostLock,
}

impl Host {
    /// Steps 3 to 10 of the startup (design 5.2.13). The logs (step 2) are
    /// T028's. Blocking I/O: run it on the main thread, within the runtime.
    pub async fn start(start: Start) -> Result<Self, StartError> {
        let Start {
            data_dir, payload, ..
        } = start;
        let injected = injected(&payload);
        let (clock, durations) = (injected.clock.clone(), injected.durations.clone());
        prepare_data_dir(&data_dir)?;
        let lock = HostLock::acquire(&data_dir)?;
        let host_db = data_dir.join("host.db");
        let mut conn = open_writer(&host_db, Database::Host).map_err(StartError::Open)?;
        migrate(&mut conn, MIGRATIONS, &data_dir.join("backups"), &*clock)?;
        let boot = epoch::current_boot_session(&payload)?;
        let run = recover::begin_run(&mut conn, boot, clock.now_wall_ms())?;

        let quiescence = Quiescence::new();
        for kind in [WRITER, RAW_WRITER, retention::MAINTENANCE] {
            quiescence.register(kind);
        }
        let config = WriterConfig::for_startup(&payload, &durations);
        let (writer, writer_thread) = Writer::spawn(conn, config, &quiescence)?;
        let mut raw =
            open_writer(&data_dir.join("raw.db"), Database::Raw).map_err(StartError::Open)?;
        prepare_raw(&mut raw, &RAW_SCHEMA)?;
        let (raw, raw_thread) = RawWriter::spawn(raw, &quiescence)?;
        let raw_sink = raw.sink();
        let readers = Readers::open(&host_db).map_err(StartError::Open)?;
        let streams_config = StreamsConfig::new(run.epoch, &durations);
        let streams = Streams::new(
            writer.clone(),
            readers.clone(),
            clock.clone(),
            streams_config,
        );
        let blobs = BlobStore::new(&data_dir, writer.clone(), clock.clone());
        let backups = data_dir.join("backups");
        let maintain = || {
            let (raw, blobs, backups) = (raw.clone(), blobs.clone(), backups.clone());
            retention::Maintenance::new(
                writer.clone(),
                raw,
                blobs,
                backups,
                clock.clone(),
                durations.clone(),
            )
        };
        let maintenance = maintain();
        // The ledger reads beside the writer, which took the write connection.
        let reader = open_reader(&host_db).map_err(StartError::Open)?;
        let ledger = Ledger::load(&reader, writer.clone(), clock.clone(), &durations)?;
        quiescence.register(SWEEPER);
        let sweeper = tokio::spawn(ledger.clone().sweeper(quiescence.enter(SWEEPER)));
        reap::previous_run(&reader, &writer, &*clock, &durations).await?;
        let maintenance = maintenance.spawn(&quiescence);
        // The runtime installer (T129) starts after the self-check is set up,
        // and reports `Failed(BundleChanged)` through `Watch::bundle_changed`.
        let (watch, watching) = Watch::new();
        let self_check = SelfCheck::new(watch.clone());
        let admission = Arc::new(Admission::default());

        // Claude's runtime and the sessions on it, in the order they stop.
        let ids = Arc::new(IdGen::new(clock.clone()));
        let login_env = injected.login_env.clone();
        let finder = Finder {
            clock: clock.clone(),
            durations: durations.clone(),
            data_dir: data_dir.clone(),
            login_env,
            #[cfg(feature = "test-harness")]
            agent: payload.test_config.agents.claude.clone(),
            #[cfg(feature = "test-harness")]
            in_process: payload.test_config.in_process.clone(),
            #[cfg(feature = "test-harness")]
            cli_root: payload.test_config.cli_root.clone(),
        };
        // Discovery runs the login shell once; the index's git and Claude's
        // launch both take this result.
        let found = finder.discover(project_roots(&reader)?).await;
        let git = git_tool(&found, &data_dir)?;
        let home = PathBuf::from(found.host.get("HOME").cloned().unwrap_or_default());
        let settings = SettingsHandle::load(&reader, writer.clone(), clock.clone(), &home)?;
        // The audit records of the gateway, the sessions, the index and the
        // host itself.
        let (records, audit) = mpsc::unbounded_channel();
        let (drain, drains) = mpsc::unbounded_channel();
        quiescence.register(audit::AUDIT);
        let participant = quiescence.enter(audit::AUDIT);
        tokio::spawn(audit::write(audit, drains, writer.clone(), participant));
        let version = env!("CARGO_PKG_VERSION").to_owned();
        let weak = cfg!(feature = "dev-auth").then_some(HostEvent::WeakAuth);
        for event in [Some(HostEvent::Started { version }), weak]
            .into_iter()
            .flatten()
        {
            record(&records, &*clock, event);
        }
        // Step 7: the index stream, before the socket.
        quiescence.register(INDEX);
        let index = IndexHandle::start(IndexDeps {
            streams: streams.clone(),
            readers: readers.clone(),
            db: writer.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
            durations: durations.clone(),
            git: git.clone(),
            quiescence: quiescence.clone(),
            host: HostUpdated {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                weak_auth: cfg!(feature = "dev-auth"),
                // The self-check (T059) and the power assertion (T156).
                pending_upgrade: false,
                power_assertion: false,
                last_exit_abnormal: run.abnormal,
            },
            requesters: Arc::new(ProcessRequesters::new(
                readers.clone(),
                &std::env::current_exe().unwrap_or_default(),
                cupper_sys::own_signing_identity().ok().flatten(),
            )),
            audit: records.clone(),
        })
        .await?;
        quiescence.register(runtime::INSTALLER);
        let installing = Arc::new(AtomicBool::new(false));
        if let Some(bundle) = BundleDir::of_this_host().ok().filter(BundleDir::is_app) {
            let (plans, registry) = installer_inputs(&payload);
            let installer = Installer {
                plans,
                bundle: Arc::new(bundle),
                dir: runtime::runtime_dir(&data_dir),
                // No adapter runs yet.
                in_use: BTreeSet::new(),
                registry,
                proxy: found.login.vars.get("HTTPS_PROXY").cloned(),
                durations: durations.clone(),
            };
            let participant = quiescence.enter(runtime::INSTALLER);
            let (clock, index) = (clock.clone(), index.clone());
            let (watch, installing) = (watch.clone(), Arc::clone(&installing));
            tokio::spawn(async move {
                let publish = |statuses: &Statuses| {
                    let busy = statuses.values().any(|s| *s == Status::Installing);
                    installing.store(busy, Ordering::Relaxed);
                    if statuses
                        .values()
                        .any(|s| *s == Status::Failed(Reason::BundleChanged))
                    {
                        watch.bundle_changed();
                    }
                    let (state, failure) = runtime::adapter_state(statuses);
                    index.runtime(state, failure);
                };
                participant.idle(installer.run(&*clock, publish)).await;
            });
        }
        let launch = finder.launch_found(&found, &settings.current()).await;
        let redactor = Redactor::for_this_machine();
        let log = AdapterLog::open(
            &log_dir(&data_dir),
            "claude",
            clock.clone(),
            redactor.clone(),
        )
        .unwrap_or_else(|_| AdapterLog::unwritten(clock.clone(), redactor));
        let cx = launch::Context {
            clock: clock.clone(),
            writer: writer.clone(),
            raw: raw_sink,
            log,
            self_check: self_check.clone(),
        };
        quiescence.register(AGENT_RUNTIME);
        let deps = RuntimeDeps {
            launch,
            cx,
            ids: ids.clone(),
            blobs: blobs.clone(),
            quiescence: quiescence.clone(),
            probe_dir: data_dir.join("probe").join("claude"),
            durations: durations.clone(),
            notes: Arc::new(index.clone()),
            rss: test_rss(&payload),
        };
        let claude = AgentRuntime::start(deps);
        let find: Find = {
            let (finder, readers, settings) = (finder.clone(), readers.clone(), settings.clone());
            Arc::new(move || {
                let (finder, readers, settings) =
                    (finder.clone(), readers.clone(), settings.current());
                Box::pin(async move {
                    let roots = readers.read(project_roots).await.unwrap_or_default();
                    finder.launch(roots, &settings).await
                })
            })
        };
        let agents = Agents::new(claude.clone(), readers.clone(), writer.clone(), find);
        let sessions = SessionManager::new(SessionDeps {
            streams: streams.clone(),
            readers: readers.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
            durations: durations.clone(),
            agents: HashMap::from([(AgentKind::Claude, Arc::new(claude.clone()) as _)]),
            quiescence: quiescence.clone(),
            audit: records.clone(),
            index: Arc::new(index.clone()),
            git,
            admission: Arc::clone(&admission),
            blobs: blobs.clone(),
        });
        sessions.reopen().await;
        let parts: Arc<[Arc<dyn Part>]> = Arc::new([
            Arc::new(sessions.clone()),
            Arc::new(claude),
            Arc::new(index.clone()),
        ]);
        let (to_supervisor, exits) = mpsc::unbounded_channel();
        let control = Arc::new(Control::new(
            to_supervisor.clone(),
            Arc::clone(&parts),
            Arc::clone(&admission),
        ));
        let mut routes = Routes::new(ledger.clone());
        routes.watched_by(&quiescence);
        // `stream/subscribe`, `stream/unsubscribe` and `history/page`.
        routes.streams(streams.clone(), readers.clone());
        control.route(&mut routes);
        #[cfg(feature = "test-harness")]
        Arc::new(harness::Harness {
            clock: injected.manual.clone(),
            quiescence: quiescence.clone(),
            maintenance: Arc::new(maintain()),
            ledger,
            seeder: seed::Seeder {
                streams,
                readers,
                ids,
                clock: clock.clone(),
                index: index.clone(),
            },
        })
        .route(&mut routes);
        #[cfg(any(feature = "test-harness", feature = "dev-auth"))]
        Arc::new(crate::diag::recording_export::RecordingExport {
            data_dir: data_dir.clone(),
            blobs: blobs.clone(),
        })
        .route(&mut routes);
        routes.blobs(blobs.clone());
        routes.sessions(Arc::new(sessions));
        let agents = Arc::new(agents);
        agents.route(&mut routes);
        let settings = Arc::new(SettingsPort::new(settings, Arc::clone(&agents))) as Arc<dyn Port>;
        routes.add::<SettingsGet>(Arc::clone(&settings));
        routes.add::<SettingsUpdate>(settings);
        routes.index(Arc::new(index.clone()));
        routes.presence(Arc::new(index.clone()));
        let config = GatewayConfig {
            host_id: run.host_id,
            data_epoch: run.epoch,
            // The desktop's code requirement comes with the Team ID (T180).
            ui_requirement: None,
            #[cfg(feature = "dev-auth")]
            dev_key: payload.dev_key.map(|key| crate::gateway::DevKey {
                key,
                launcher: start.launcher,
            }),
            #[cfg(feature = "test-harness")]
            harness_key: payload.harness_key,
        };
        let audit = records.clone();
        let gateway = Gateway::new(config, clock.clone(), durations.clone(), routes, records);
        control.serve_for(&gateway);
        index.serve_for(&gateway);
        if let Some(check) = self_check {
            gateway.check_self_with(check);
        }
        watching.start(WatchDeps {
            clock: clock.clone(),
            durations: durations.clone(),
            quiescence: quiescence.clone(),
            parts: Arc::clone(&parts),
            gateway: Arc::downgrade(&gateway),
            index: index.clone(),
            admission,
            exits: to_supervisor,
            parent: cfg!(feature = "dev-auth").then(std::os::unix::process::parent_id),
            installing,
            audit: audit.clone(),
        });
        let terminate = signal(SignalKind::terminate())?;
        let (listener, socket) = lock.bind()?;
        let serving = tokio::spawn(Arc::clone(&gateway).serve(listener));
        Ok(Self {
            clock,
            writer,
            writer_thread,
            raw_thread,
            parts,
            gateway,
            serving,
            sweeps: [maintenance, sweeper],
            audit,
            drain: audit::Drain(drain),
            exits,
            terminate,
            socket,
            #[cfg(all(test, feature = "test-harness"))]
            quiescence,
            _lock: lock,
        })
    }

    /// Serves until the orderly exit, which it then runs; the exit code.
    pub async fn run(mut self) -> HostExit {
        let exit = {
            let mut requested = pin!(self.exits.recv());
            let mut terminated = pin!(self.terminate.recv());
            let mut died = pin!(self.writer_thread.ended());
            poll_fn(|cx| {
                if let Poll::Ready(exit) = requested.as_mut().poll(cx) {
                    // The control port lives as long as the gateway.
                    return Poll::Ready(exit.unwrap_or(Exit::Terminated));
                }
                if terminated.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Exit::Terminated);
                }
                died.as_mut().poll(cx).map(|()| Exit::WriterDied)
            })
            .await
        };
        self.exit(exit).await
    }

    /// The orderly exit (design 5.2.13). The connections close and the
    /// audit is stored before the store writer stops, so `conn.closed` and
    /// `host.exiting` reach the `audit` table.
    async fn exit(self, exit: Exit) -> HostExit {
        self.serving.abort();
        let reason = shutdown_reason(exit);
        tracing::info!(?exit, reason = reason.as_str(), "orderly exit");
        self.gateway.broadcast::<HostShutdown>(&HostShutdownParams {
            reason,
            restart_expected: false,
        });
        // The watch's exits come only once no turn runs.
        let force = matches!(
            exit,
            Exit::Requested { force: true } | Exit::Terminated | Exit::WriterDied
        );
        let reason = exit.code().to_owned();
        record(
            &self.audit,
            &*self.clock,
            HostEvent::Exiting { reason, force },
        );
        for part in self.parts.iter() {
            part.stop(force).await;
        }
        for sweep in self.sweeps {
            sweep.abort();
            let _ = sweep.await;
        }
        self.gateway.close_all().await;
        self.drain.wait().await;
        // The last write before the writer stops: the next start reads a
        // clean exit (design 5.2.8).
        let mut code = HostExit::Ok;
        if exit == Exit::WriterDied {
            code = HostExit::Error;
        } else {
            let at = self.clock.now_wall_ms().to_string();
            let clean = self.writer.submit(Durability::Normal, move |conn| {
                meta::set(conn, MetaKey::LastCleanExitAt, &at)
            });
            if clean.await.is_err() {
                code = HostExit::Error;
            }
        }
        let (raw_thread, writer_thread) = (self.raw_thread, self.writer_thread);
        let stopped = tokio::task::spawn_blocking(move || {
            raw_thread.stop().is_ok() & writer_thread.stop().is_ok()
        });
        if !matches!(stopped.await, Ok(true)) {
            code = HostExit::Error;
        }
        let _ = std::fs::remove_file(&self.socket);
        code
    }
}

impl Exit {
    /// The exit's code in `host.exiting`.
    fn code(self) -> &'static str {
        match self {
            Self::Requested { .. } => "requested",
            Self::Terminated => "terminated",
            Self::WriterDied => "writer_died",
            Self::Idle => "idle",
            Self::Orphaned => "orphaned",
            Self::Upgrade => "upgrade",
        }
    }
}

/// Sends `event` into the audit (design 5.8.6).
fn record(audit: &mpsc::UnboundedSender<AuditRecord>, clock: &dyn Clock, event: HostEvent) {
    let (at_ms, conn, event) = (clock.now_wall_ms(), None, AuditEvent::Host(event));
    // Nobody listening is the receiver's choice.
    let _ = audit.send(AuditRecord { at_ms, conn, event });
}

/// `host/shutdown`'s reason: SIGTERM, the orphaned state and a dead writer
/// take `user_quit` too (ruled before T037).
fn shutdown_reason(exit: Exit) -> ShutdownReason {
    match exit {
        Exit::Idle => ShutdownReason::Idle,
        Exit::Upgrade => ShutdownReason::Upgrade,
        _ => ShutdownReason::UserQuit,
    }
}

/// CLI discovery (design 5.6.6, 5.2.10): the host's own environment, the
/// login shell's (or `test_config.login_env`), and a locator on the trusted
/// entries of its `PATH`, outside every project. The git tool comes from the
/// same locator.
#[derive(Debug)]
struct Discovery {
    host: BTreeMap<String, String>,
    login: LoginEnv,
    locator: CliLocator,
}

async fn discover(finder: &Finder, roots: Vec<PathBuf>) -> Discovery {
    let host = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    let host: BTreeMap<String, String> = host.collect();
    let (clock, durations) = (&*finder.clock, &finder.durations);
    let login = launch::login_env(clock, durations, &host, finder.login_env.clone()).await;
    let tmp = host.get("TMPDIR").map(String::as_str);
    let boundaries = Boundaries::new(roots, system_temp_dirs(tmp));
    let path = login.vars.get("PATH").map_or("", String::as_str);
    let home = PathBuf::from(host.get("HOME").cloned().unwrap_or_default());
    let locator = CliLocator::new(boundaries, path, home);
    #[cfg(feature = "test-harness")]
    let locator = locator.under(finder.cli_root.clone());
    Discovery {
        host,
        login,
        locator,
    }
}

/// The git tool (T034), from the same locator: `None` when no trusted `PATH`
/// entry has a git.
fn git_tool(found: &Discovery, data_dir: &Path) -> Result<Option<GitTool>, StartError> {
    let locator = &found.locator;
    let Some(git) = locator.tool("git") else {
        return Ok(None);
    };
    let (lfs, trusted) = (locator.tool("git-lfs"), locator.trusted_path());
    let home = PathBuf::from(found.host.get("HOME").cloned().unwrap_or_default());
    let ssh = found.login.vars.get("SSH_AUTH_SOCK").map(OsString::from);
    Ok(Some(GitTool::new(
        git,
        lfs,
        trusted,
        Some(home),
        ssh,
        data_dir,
    )?))
}

/// The projects' paths, the boundaries of trusted `PATH` entries.
fn project_roots(reader: &rusqlite::Connection) -> rusqlite::Result<Vec<PathBuf>> {
    let mut projects = reader.prepare("SELECT path FROM projects WHERE removed_at IS NULL")?;
    let roots = projects.query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    roots.collect()
}

/// How Claude's adapters start, found at startup and again on
/// `agents/refresh`: the program of `test_config.agents`, else the runtime
/// with the CLI discovered.
#[derive(Debug, Clone)]
struct Finder {
    clock: Arc<dyn Clock>,
    durations: Durations,
    data_dir: PathBuf,
    login_env: Option<BTreeMap<String, String>>,
    #[cfg(feature = "test-harness")]
    agent: Option<crate::startup::test_config::AgentCommand>,
    #[cfg(feature = "test-harness")]
    in_process: Option<launch::InProcess>,
    #[cfg(feature = "test-harness")]
    cli_root: Option<PathBuf>,
}

impl Finder {
    async fn discover(&self, roots: Vec<PathBuf>) -> Discovery {
        discover(self, roots).await
    }

    /// Discovers again, then finds the launch: what `agents/refresh` does.
    async fn launch(&self, roots: Vec<PathBuf>, settings: &Settings) -> Launch {
        let found = self.discover(roots).await;
        self.launch_found(&found, settings).await
    }

    async fn launch_found(&self, found: &Discovery, settings: &Settings) -> Launch {
        let (clock, durations) = (&*self.clock, &self.durations);
        let (host, login) = (&found.host, &found.login);
        #[cfg(feature = "test-harness")]
        if let Some(agents) = &self.in_process {
            let mut launch = launch::injected(PathBuf::new(), Vec::new(), host, login);
            launch.program = launch::Program::InProcess(agents.clone());
            return launch;
        }
        #[cfg(feature = "test-harness")]
        if let Some(agent) = &self.agent {
            let (program, args) = (agent.program.clone(), agent.args.clone());
            let mut launch = launch::injected(program, args, host, login);
            if let Some(version) = &agent.cli_version {
                let found = crate::driver::probe::version::Version::find(version);
                launch.versions = launch::cli_versions(found);
            }
            return launch;
        }
        let (dir, locator) = (&self.data_dir, &found.locator);
        launch::production(clock, durations, dir, (host, login), locator, settings).await
    }
}

/// The runtime's plans and registry: the embedded manifest's and npm's, or
/// `test_config.runtime`'s (design 5.9.9).
fn installer_inputs(payload: &Payload) -> (Vec<ComponentPlan>, String) {
    let embedded = || runtime::plans(manifest::embedded());
    #[cfg(feature = "test-harness")]
    {
        let config = &payload.test_config.runtime;
        let read = |path: &PathBuf| serde_json::from_slice(&std::fs::read(path).ok()?).ok();
        let plans = config.manifest.as_ref().and_then(read);
        let plans = plans.map(|manifest| runtime::plans(&manifest));
        let registry = config.registry_url.clone();
        (
            plans.unwrap_or_else(embedded),
            registry.unwrap_or_else(|| npm::REGISTRY.to_owned()),
        )
    }
    #[cfg(not(feature = "test-harness"))]
    {
        let _ = payload;
        (embedded(), npm::REGISTRY.to_owned())
    }
}

/// `test_config.rss_samples` of Claude (design 5.9.9).
fn test_rss(payload: &Payload) -> Option<u64> {
    #[cfg(feature = "test-harness")]
    return payload.test_config.rss_samples.claude;
    #[cfg(not(feature = "test-harness"))]
    {
        let _ = payload;
        None
    }
}

/// The clock, the durations and the login shell's environment of
/// `test_config` (design 5.9.9), or of production.
fn injected(payload: &Payload) -> Injected {
    #[cfg(feature = "test-harness")]
    {
        use crate::startup::test_config::ClockKind;
        let manual = (payload.test_config.clock == ClockKind::Manual).then(ManualClock::new);
        let clock: Arc<dyn Clock> = match &manual {
            Some(manual) => Arc::new(manual.clone()),
            None => Arc::new(SystemClock::new()),
        };
        Injected {
            clock,
            durations: payload.test_config.durations.clone(),
            login_env: payload.test_config.login_env.clone(),
            manual,
        }
    }
    #[cfg(not(feature = "test-harness"))]
    {
        let _ = payload;
        Injected {
            clock: Arc::new(SystemClock::new()),
            durations: Durations::default(),
            login_env: None,
        }
    }
}

/// What `test_config` (design 5.9.9) sets, or production.
struct Injected {
    clock: Arc<dyn Clock>,
    durations: Durations,
    /// Stands in for the login shell.
    login_env: Option<BTreeMap<String, String>>,
    /// The clock `test/clock_advance` moves, when the host runs on one.
    #[cfg(feature = "test-harness")]
    manual: Option<ManualClock>,
}

/// Why the host could not start, with its exit code (design 5.2.13).
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    #[error(transparent)]
    DataDir(#[from] DataDirError),
    #[error(transparent)]
    Listen(#[from] ListenError),
    #[error("cannot open a database")]
    Open(#[source] OpenError),
    #[error(transparent)]
    Migrate(#[from] MigrateError),
    #[error("cannot read or write a database")]
    Sql(#[from] rusqlite::Error),
    #[error("meta.{} is not valid", .0.name())]
    Meta(MetaKey),
    #[error(transparent)]
    Ledger(#[from] LedgerError),
    #[error(transparent)]
    Boot(#[from] BootSessionError),
    #[error("cannot load the index stream")]
    Index(#[from] IndexError),
    #[error("cannot start a thread or the signal handler")]
    Io(#[from] std::io::Error),
}

impl StartError {
    pub fn exit(&self) -> HostExit {
        match self {
            Self::Listen(err) => err.exit(),
            Self::Migrate(MigrateError::TooNew { .. }) => HostExit::DataTooNew,
            Self::Migrate(MigrateError::Failed { .. }) => HostExit::MigrationFailed,
            Self::DataDir(_)
            | Self::Open(_)
            | Self::Migrate(_)
            | Self::Sql(_)
            | Self::Meta(_)
            | Self::Ledger(_)
            | Self::Index(_) => HostExit::DataDirUnusable,
            Self::Boot(_) | Self::Io(_) => HostExit::Error,
        }
    }
}

#[cfg(all(test, feature = "test-harness"))]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::gateway::testkit::TempDir;

    /// The kinds of actor the source declares (`ActorKind::new("<name>")`).
    fn declared() -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let mut dirs = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for path in std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
            {
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap_or_default();
                for (_, rest) in text
                    .match_indices("ActorKind::new(\"")
                    .map(|(at, _)| text.split_at(at + 16))
                {
                    let name = rest.split('"').next().unwrap();
                    if name.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                        names.insert(name.to_owned());
                    }
                }
            }
        }
        names
    }

    /// A kind of actor the host does not register would escape the barrier.
    #[tokio::test(flavor = "multi_thread")]
    async fn it_clock_advance_the_host_registers_every_kind_of_actor() {
        let dir = TempDir::new("kinds");
        let mut payload = Payload::default();
        // The fake agent, never started: no CLI discovery in the test's own
        // process, whose `HOME` is the user's (T203).
        let agent = serde_json::json!({"program": launch::fake_agent()});
        payload.test_config.agents.claude = Some(serde_json::from_value(agent).unwrap());
        let path = BTreeMap::from([("PATH".to_owned(), "/usr/bin:/bin".to_owned())]);
        payload.test_config.login_env = Some(path);
        let start = Start {
            data_dir: dir.path().join("data"),
            payload,
            #[cfg(feature = "dev-auth")]
            launcher: 0,
        };
        let host = Host::start(start).await.unwrap();
        let kinds = host.quiescence.kinds();
        let registered: BTreeSet<String> = kinds.iter().map(|k| k.name().to_owned()).collect();
        assert_eq!(registered, declared());
    }
}
