//! The host's lifecycle (design 5.2.13): [`Host::start`] runs startup steps
//! 3 to 10, and [`Host::run`] serves until one of the triggers of the orderly
//! exit comes, then exits.
//!
//! The orderly exit comes on `host/exit` ([`control`]), on SIGTERM, and when
//! the store writer has died (T024 review: the host cannot run without it).
//! Idle exit, the orphaned state, the self-check, the cleanup of leftover
//! adapters and the audit table are T059's.

pub mod control;
mod recover;

use std::collections::{BTreeMap, HashMap};
use std::ffi::OsString;
use std::fmt;
use std::future::poll_fn;
use std::path::{Path, PathBuf};
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;

use cupper_protocol::domain::AgentKind;
use cupper_protocol::index::HostUpdated;
use cupper_protocol::methods::handshake::{HostShutdown, HostShutdownParams, ShutdownReason};
use futures::future::BoxFuture;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use self::control::Control;
use crate::clock::{Clock, IdGen, SystemClock};
use crate::diag::full::Redactor;
use crate::diag::log::log_dir;
use crate::diag::stderr::AdapterLog;
use crate::driver::locate::CliLocator;
use crate::driver::locate::trust::{Boundaries, system_temp_dirs};
use crate::driver::probe::login_shell::LoginEnv;
use crate::driver::runtime::agents::{Agents, Find};
use crate::driver::runtime::launch::{self, Launch};
use crate::driver::runtime::status::NoAgentNotes;
use crate::driver::runtime::{AGENT_RUNTIME, AgentRuntime, RuntimeDeps};
use crate::durations::Durations;
use crate::exit::HostExit;
use crate::gateway::listen::{HostLock, ListenError};
use crate::gateway::{Gateway, GatewayConfig, Routes};
use crate::git::GitTool;
use crate::index::{INDEX, IndexDeps, IndexError, IndexHandle};
use crate::ledger::{Ledger, LedgerError};
use crate::orchestrator::session::{SessionDeps, SessionManager};
use crate::quiescence::Quiescence;
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
    exits: mpsc::UnboundedReceiver<Exit>,
    terminate: Signal,
    socket: PathBuf,
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
        let (clock, durations, login_env) = injected(&payload);
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
        let maintenance = retention::Maintenance::new(
            writer.clone(),
            raw,
            blobs.clone(),
            backups,
            clock.clone(),
            durations.clone(),
        );
        // The ledger reads beside the writer, which took the write connection.
        let reader = open_reader(&host_db).map_err(StartError::Open)?;
        let ledger = Ledger::load(&reader, writer.clone(), clock.clone(), &durations)?;
        let sweeper = tokio::spawn(ledger.clone().sweeper());
        let maintenance = maintenance.spawn(&quiescence);

        // Claude's runtime and the sessions on it, in the order they stop.
        let ids = Arc::new(IdGen::new(clock.clone()));
        let finder = Finder {
            clock: clock.clone(),
            durations: durations.clone(),
            data_dir: data_dir.clone(),
            login_env,
            #[cfg(feature = "test-harness")]
            agent: (payload.test_config.agents.claude.as_ref())
                .map(|agent| (agent.program.clone(), agent.args.clone())),
        };
        // Discovery runs the login shell once; the index's git and Claude's
        // launch both take this result.
        let found = finder.discover(project_roots(&reader)?).await;
        let git = git_tool(&found, &data_dir)?;
        // Step 7: the index stream, before the socket.
        quiescence.register(INDEX);
        let index = IndexHandle::start(IndexDeps {
            streams: streams.clone(),
            readers: readers.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
            durations: durations.clone(),
            git,
            quiescence: quiescence.clone(),
            host: HostUpdated {
                version: env!("CARGO_PKG_VERSION").to_owned(),
                weak_auth: cfg!(feature = "dev-auth"),
                // The self-check (T059) and the power assertion (T156).
                pending_upgrade: false,
                power_assertion: false,
                last_exit_abnormal: run.abnormal,
            },
        })
        .await?;
        let launch = finder.launch_found(&found).await;
        let redactor = Redactor::for_this_machine();
        let log = AdapterLog::open(&log_dir(&data_dir), "claude", clock.clone(), redactor);
        let cx = launch::Context {
            clock: clock.clone(),
            writer: writer.clone(),
            raw: raw_sink,
            log: log.ok(),
        };
        quiescence.register(AGENT_RUNTIME);
        let deps = RuntimeDeps {
            launch,
            cx,
            ids: ids.clone(),
            blobs,
            quiescence: quiescence.clone(),
            probe_dir: data_dir.join("probe").join("claude"),
            durations: durations.clone(),
            notes: Arc::new(NoAgentNotes),
        };
        let claude = AgentRuntime::start(deps);
        let find: Find = {
            let (finder, readers) = (finder.clone(), readers.clone());
            Arc::new(move || {
                let (finder, readers) = (finder.clone(), readers.clone());
                Box::pin(async move {
                    let roots = readers.read(project_roots).await.unwrap_or_default();
                    finder.launch(roots).await
                })
            })
        };
        let agents = Agents::new(claude.clone(), readers.clone(), writer.clone(), find);
        // The audit records go into the `audit` table from T059 on.
        let (records, _audit) = mpsc::unbounded_channel();
        let sessions = SessionManager::new(SessionDeps {
            streams,
            writer: writer.clone(),
            readers,
            clock: clock.clone(),
            ids,
            durations: durations.clone(),
            agents: HashMap::from([(AgentKind::Claude, Arc::new(claude.clone()) as _)]),
            quiescence: quiescence.clone(),
            audit: records.clone(),
            index: Arc::new(index.clone()),
        });
        let parts: Arc<[Arc<dyn Part>]> = Arc::new([
            Arc::new(sessions.clone()),
            Arc::new(claude),
            Arc::new(index.clone()),
        ]);
        let (to_supervisor, exits) = mpsc::unbounded_channel();
        let control = Arc::new(Control::new(to_supervisor, Arc::clone(&parts)));
        let mut routes = Routes::new(ledger);
        control.route(&mut routes);
        routes.sessions(Arc::new(sessions));
        Arc::new(agents).route(&mut routes);
        routes.index(Arc::new(index.clone()));
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
        let gateway = Gateway::new(config, clock.clone(), durations, routes, records);
        control.serve_for(&gateway);
        index.serve_for(&gateway);
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
            exits,
            terminate,
            socket,
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

    /// The orderly exit (design 5.2.13).
    async fn exit(self, exit: Exit) -> HostExit {
        self.serving.abort();
        self.gateway.broadcast::<HostShutdown>(&HostShutdownParams {
            reason: ShutdownReason::UserQuit,
            restart_expected: false,
        });
        let force = exit != (Exit::Requested { force: false });
        for part in self.parts.iter() {
            part.stop(force).await;
        }
        for sweep in self.sweeps {
            sweep.abort();
            let _ = sweep.await;
        }
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
        self.gateway.close_all().await;
        let _ = std::fs::remove_file(&self.socket);
        code
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

async fn discover(
    login_env: Option<BTreeMap<String, String>>,
    clock: &dyn Clock,
    durations: &Durations,
    roots: Vec<PathBuf>,
) -> Discovery {
    let host = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    let host: BTreeMap<String, String> = host.collect();
    let login = launch::login_env(clock, durations, &host, login_env).await;
    let tmp = host.get("TMPDIR").map(String::as_str);
    let boundaries = Boundaries::new(roots, system_temp_dirs(tmp));
    let path = login.vars.get("PATH").map_or("", String::as_str);
    let home = PathBuf::from(host.get("HOME").cloned().unwrap_or_default());
    let locator = CliLocator::new(boundaries, path, home);
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
    agent: Option<(PathBuf, Vec<String>)>,
}

impl Finder {
    async fn discover(&self, roots: Vec<PathBuf>) -> Discovery {
        discover(self.login_env.clone(), &*self.clock, &self.durations, roots).await
    }

    /// Discovers again, then finds the launch: what `agents/refresh` does.
    async fn launch(&self, roots: Vec<PathBuf>) -> Launch {
        let found = self.discover(roots).await;
        self.launch_found(&found).await
    }

    async fn launch_found(&self, found: &Discovery) -> Launch {
        let (clock, durations) = (&*self.clock, &self.durations);
        let (host, login) = (&found.host, &found.login);
        #[cfg(feature = "test-harness")]
        if let Some((program, args)) = &self.agent {
            return launch::injected(program.clone(), args.clone(), host, login);
        }
        let (dir, locator) = (&self.data_dir, &found.locator);
        launch::production(clock, durations, dir, host, login, locator).await
    }
}

/// The clock, the durations and the login shell's environment of
/// `test_config` (design 5.9.9), or of production.
fn injected(payload: &Payload) -> (Arc<dyn Clock>, Durations, Option<BTreeMap<String, String>>) {
    #[cfg(feature = "test-harness")]
    {
        use crate::startup::test_config::ClockKind;
        let clock: Arc<dyn Clock> = match payload.test_config.clock {
            ClockKind::System => Arc::new(SystemClock::new()),
            ClockKind::Manual => Arc::new(crate::clock::ManualClock::new()),
        };
        let login_env = payload.test_config.login_env.clone();
        (clock, payload.test_config.durations.clone(), login_env)
    }
    #[cfg(not(feature = "test-harness"))]
    {
        let _ = payload;
        (Arc::new(SystemClock::new()), Durations::default(), None)
    }
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
