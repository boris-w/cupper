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

use std::fmt;
use std::future::poll_fn;
use std::path::PathBuf;
use std::pin::pin;
use std::sync::Arc;
use std::task::Poll;

use cupper_protocol::methods::handshake::{HostShutdown, HostShutdownParams, ShutdownReason};
use futures::future::BoxFuture;
use tokio::signal::unix::{Signal, SignalKind, signal};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use self::control::Control;
use crate::clock::{Clock, SystemClock};
use crate::durations::Durations;
use crate::exit::HostExit;
use crate::gateway::listen::{HostLock, ListenError};
use crate::gateway::{Gateway, GatewayConfig, Routes};
use crate::ledger::{Ledger, LedgerError};
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
    /// The stream registry, which the session and index actors (T043, T034)
    /// write and read through.
    #[expect(
        dead_code,
        reason = "the session and index actors (T043, T034) take it"
    )]
    streams: Streams,
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
    /// T028's, the index stream (step 7) T034's. Blocking I/O: run it on the
    /// main thread, within the runtime.
    pub async fn start(start: Start) -> Result<Self, StartError> {
        let Start {
            data_dir, payload, ..
        } = start;
        let (clock, durations) = injected(&payload);
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
        // `raw.sink()` goes to the drivers' taps (T055).
        let (raw, raw_thread) = RawWriter::spawn(raw, &quiescence)?;
        let readers = Readers::open(&host_db).map_err(StartError::Open)?;
        let streams_config = StreamsConfig::new(run.epoch, &durations);
        let streams = Streams::new(writer.clone(), readers, clock.clone(), streams_config);
        let blobs = BlobStore::new(&data_dir, writer.clone(), clock.clone());
        let backups = data_dir.join("backups");
        let maintenance = retention::Maintenance::new(
            writer.clone(),
            raw,
            blobs,
            backups,
            clock.clone(),
            durations.clone(),
        );
        // The ledger reads beside the writer, which took the write connection.
        let reader = open_reader(&host_db).map_err(StartError::Open)?;
        let ledger = Ledger::load(&reader, writer.clone(), clock.clone(), &durations)?;
        let sweeper = tokio::spawn(ledger.clone().sweeper());
        let maintenance = maintenance.spawn(&quiescence);

        let parts: Arc<[Arc<dyn Part>]> = Arc::new([]);
        let (to_supervisor, exits) = mpsc::unbounded_channel();
        let control = Arc::new(Control::new(to_supervisor, Arc::clone(&parts)));
        let mut routes = Routes::new(ledger);
        control.route(&mut routes);
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
        // The audit records go into the `audit` table from T059 on.
        let (gateway, _audit) = Gateway::new(config, clock.clone(), durations, routes);
        control.serve_for(&gateway);
        let terminate = signal(SignalKind::terminate())?;
        let (listener, socket) = lock.bind()?;
        let serving = tokio::spawn(Arc::clone(&gateway).serve(listener));
        Ok(Self {
            clock,
            writer,
            writer_thread,
            raw_thread,
            streams,
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

/// The clock and the durations of `test_config` (design 5.9.9), or of
/// production.
fn injected(payload: &Payload) -> (Arc<dyn Clock>, Durations) {
    #[cfg(feature = "test-harness")]
    {
        use crate::startup::test_config::ClockKind;
        let clock: Arc<dyn Clock> = match payload.test_config.clock {
            ClockKind::System => Arc::new(SystemClock::new()),
            ClockKind::Manual => Arc::new(crate::clock::ManualClock::new()),
        };
        (clock, payload.test_config.durations.clone())
    }
    #[cfg(not(feature = "test-harness"))]
    {
        let _ = payload;
        (Arc::new(SystemClock::new()), Durations::default())
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
            | Self::Ledger(_) => HostExit::DataDirUnusable,
            Self::Boot(_) | Self::Io(_) => HostExit::Error,
        }
    }
}
