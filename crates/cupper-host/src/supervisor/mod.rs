//! The host's lifecycle (design 5.2.13): [`Host::start`] runs startup steps
//! 3 to 10, and [`Host::run`] serves until one of the triggers of the orderly
//! exit comes, then exits.
//!
//! The orderly exit comes on `host/exit` ([`control`]), on SIGTERM, and when
//! the store writer has died (T024 review: the host cannot run without it).
//! Idle exit, the orphaned state, the self-check, the cleanup of leftover
//! adapters and the audit table are T059's.

pub mod control;
#[cfg(feature = "test-harness")]
mod harness;
mod recover;
#[cfg(feature = "test-harness")]
mod seed;

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
#[cfg(feature = "test-harness")]
use crate::clock::ManualClock;
use crate::clock::{Clock, IdGen, SystemClock};
use crate::diag::full::Redactor;
use crate::diag::log::log_dir;
use crate::diag::stderr::AdapterLog;
use crate::driver::locate::CliLocator;
use crate::driver::locate::trust::{Boundaries, system_temp_dirs};
use crate::driver::probe::login_shell::LoginEnv;
use crate::driver::runtime::launch::{self, Launch};
use crate::driver::runtime::{AGENT_RUNTIME, AgentRuntime, RuntimeDeps};
use crate::durations::Durations;
use crate::exit::HostExit;
use crate::gateway::listen::{HostLock, ListenError};
use crate::gateway::{Gateway, GatewayConfig, Routes};
use crate::git::GitTool;
use crate::index::{INDEX, IndexDeps, IndexError, IndexHandle};
use crate::ledger::{Ledger, LedgerError, SWEEPER};
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
        let maintenance = maintenance.spawn(&quiescence);

        // Claude's runtime and the sessions on it, in the order they stop.
        let ids = Arc::new(IdGen::new(clock.clone()));
        let login_env = injected.login_env.clone();
        let found = discover(login_env, &*clock, &durations, &reader, &data_dir).await?;
        // Step 7: the index stream, before the socket.
        quiescence.register(INDEX);
        let index = IndexHandle::start(IndexDeps {
            streams: streams.clone(),
            readers: readers.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
            durations: durations.clone(),
            git: found.git.clone(),
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
        let launch = claude_launch(&payload, &*clock, &durations, &data_dir, &found).await;
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
            blobs: blobs.clone(),
            quiescence: quiescence.clone(),
        };
        let claude = AgentRuntime::start(deps);
        // The audit records go into the `audit` table from T059 on.
        let (records, _audit) = mpsc::unbounded_channel();
        let sessions = SessionManager::new(SessionDeps {
            streams: streams.clone(),
            writer: writer.clone(),
            readers: readers.clone(),
            clock: clock.clone(),
            ids: ids.clone(),
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
        let mut routes = Routes::new(ledger.clone());
        routes.watched_by(&quiescence);
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
        routes.sessions(Arc::new(sessions));
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
    /// `None` when no trusted `PATH` entry has a git.
    git: Option<GitTool>,
}

async fn discover(
    login_env: Option<BTreeMap<String, String>>,
    clock: &dyn Clock,
    durations: &Durations,
    reader: &rusqlite::Connection,
    data_dir: &Path,
) -> Result<Discovery, StartError> {
    let host = std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)));
    let host: BTreeMap<String, String> = host.collect();
    let login = launch::login_env(clock, durations, &host, login_env).await;
    let mut projects = reader.prepare("SELECT path FROM projects WHERE removed_at IS NULL")?;
    let roots = projects.query_map([], |row| Ok(PathBuf::from(row.get::<_, String>(0)?)))?;
    let tmp = host.get("TMPDIR").map(String::as_str);
    let roots: Vec<PathBuf> = roots.collect::<Result<_, _>>()?;
    let boundaries = Boundaries::new(roots, system_temp_dirs(tmp));
    let path = login.vars.get("PATH").map_or("", String::as_str);
    let home = PathBuf::from(host.get("HOME").cloned().unwrap_or_default());
    let locator = CliLocator::new(boundaries, path, home.clone());
    let git = match locator.tool("git") {
        Some(git) => {
            let (lfs, trusted) = (locator.tool("git-lfs"), locator.trusted_path());
            let ssh = login.vars.get("SSH_AUTH_SOCK").map(OsString::from);
            Some(GitTool::new(git, lfs, trusted, Some(home), ssh, data_dir)?)
        }
        None => None,
    };
    Ok(Discovery {
        host,
        login,
        locator,
        git,
    })
}

/// How Claude's adapters start: the program of `test_config.agents`, else
/// the runtime with the CLI discovered.
async fn claude_launch(
    payload: &Payload,
    clock: &dyn Clock,
    durations: &Durations,
    data_dir: &Path,
    found: &Discovery,
) -> Launch {
    let (host, login) = (&found.host, &found.login);
    #[cfg(feature = "test-harness")]
    if let Some(agent) = &payload.test_config.agents.claude {
        let (program, args) = (agent.program.clone(), agent.args.clone());
        return launch::injected(program, args, host, login);
    }
    #[cfg(not(feature = "test-harness"))]
    let _ = payload;
    launch::production(clock, durations, data_dir, host, login, &found.locator).await
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
