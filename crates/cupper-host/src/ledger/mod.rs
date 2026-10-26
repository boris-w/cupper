//! The command ledger (design 5.2.5): a command runs once per
//! `(principal, command_id)`, however often a client sends it.
//!
//! Commands are the methods whose idempotency is `command` (design 5.3.4).
//! The key is the principal's stable ID (`pr_desktop`, `pr_cli`, ...) and the
//! client's `command_id`, so a retry after a reconnect still finds the first
//! run. The ledger also keeps the [`params_hash`]: a repeat with other
//! parameters, or of another method, is `idempotency_mismatch`.
//!
//! A command the ledger knows is in one of three states:
//!
//! - running (`inflight`, in memory only): a repeat waits for the first run's
//!   outcome ([`Repeat::Wait`]);
//! - done: a repeat gets that outcome again, with `replayed: true`
//!   ([`Repeat::Replay`]);
//! - outcome unknown: the command recorded `pending_external` before it
//!   touched the user's repository ([`Ticket::mark_external`]), and its
//!   outcome never was. A repeat gets `conflict/outcome_unknown`; the client
//!   refreshes and the user decides.
//!
//! The gateway calls [`Ledger::begin`] in the read loop, which answers from
//! memory without waiting for the database. A first run gets a [`Ticket`] that
//! goes with the command to its executor, which ends it one of three ways:
//!
//! - [`Ticket::complete`]: the [`Completion`]'s row commits in the
//!   transaction of the command's first events, through `Batch::with`
//!   (design 5.2.3, rule 3), or on its own with [`Completion::commit`] for a
//!   command without events. Once committed, a repeat replays the outcome.
//! - [`Ticket::reject`]: the command failed before it ran (bad parameters, a
//!   precondition). Nothing is recorded, and a retry runs it again.
//! - dropping the ticket or the completion: as if the host had stopped there.
//!
//! The ledger keeps the commands of the last `ledger_retention` (48 hours) in
//! memory, loaded from the `commands` table at startup, and in the table.
//! [`Ledger::sweeper`] drops older ones at startup and every `ledger_sweep`
//! (design 5.4.5).

mod hash;
mod row;
#[cfg(test)]
mod tests;
mod ticket;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cupper_protocol::canonical::CanonicalError;
use cupper_protocol::error::{ErrorCode, ErrorReason, INVALID_PARAMS, RpcError};
use cupper_protocol::ids::{CommandId, Digest, PrincipalId};
use cupper_protocol::rpc::REPLAYED;
use rusqlite::Connection;
use serde_json::Value;
use tokio::sync::watch;

pub use hash::params_hash;
pub use ticket::{Completion, Ticket, Waiter};

use crate::clock::Clock;
use crate::durations::{Durations, LEDGER_RETENTION, LEDGER_SWEEP};
use crate::quiescence::{ActorKind, Participant};
use crate::store::{Durability, WriteError, Writer};

/// The sweeper's kind in the quiescence barrier.
pub const SWEEPER: ActorKind = ActorKind::new("ledger_sweeper");

/// A command's answer: its result, or the error it ended with.
pub type Outcome = Result<Value, RpcError>;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// `idempotency_mismatch`.
    #[error("the command_id was used before with another method or other parameters")]
    Mismatch,
    /// `conflict/outcome_unknown`.
    #[error("the command may have changed the repository, but its outcome was not recorded")]
    OutcomeUnknown,
    /// Invalid params: they have no canonical form.
    #[error("the parameters have no canonical form")]
    Params(#[from] CanonicalError),
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error("the commands table holds a bad row: {0}")]
    Corrupt(String),
}

impl From<LedgerError> for RpcError {
    fn from(err: LedgerError) -> Self {
        let message = err.to_string();
        match err {
            LedgerError::Mismatch => RpcError::app(ErrorCode::IdempotencyMismatch, message),
            LedgerError::OutcomeUnknown => {
                RpcError::app(ErrorCode::Conflict, message).with_reason(ErrorReason::OutcomeUnknown)
            }
            LedgerError::Params(_) => RpcError::new(INVALID_PARAMS, message),
            LedgerError::Write(_) | LedgerError::Sql(_) | LedgerError::Corrupt(_) => {
                RpcError::app(ErrorCode::Internal, message)
            }
        }
    }
}

/// What [`Ledger::begin`] decides.
#[derive(Debug)]
pub enum Begin {
    /// The first run: run the command and end the ticket.
    Run(Ticket),
    Repeat(Repeat),
}

/// What a repeat of a command the ledger knows gets.
#[derive(Debug)]
pub enum Repeat {
    /// The first run's outcome, with `replayed: true`.
    Replay(Outcome),
    /// The first run is still going: its outcome, once it has one.
    Wait(Waiter),
}

/// The ledger. Clones share it; the host has one.
#[derive(Debug, Clone)]
pub struct Ledger {
    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    /// Held for lookups and updates only, never during I/O.
    table: Mutex<HashMap<Key, Entry>>,
    writer: Writer,
    clock: Arc<dyn Clock>,
    retention: Duration,
    sweep_every: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Key {
    principal: PrincipalId,
    command: CommandId,
}

impl Key {
    fn new(principal: &PrincipalId, command: &CommandId) -> Self {
        let (principal, command) = (principal.clone(), command.clone());
        Self { principal, command }
    }
}

/// A command as it began: what its row records besides the outcome.
#[derive(Debug, Clone)]
struct Begun {
    method: Box<str>,
    hash: Digest,
    /// Wall-clock milliseconds.
    created_at: u64,
}

#[derive(Debug)]
struct Entry {
    begun: Begun,
    state: State,
}

#[derive(Debug)]
enum State {
    /// Running in this host; the receiver gets its outcome.
    Running(watch::Receiver<Option<Outcome>>),
    Done(Outcome),
    Unknown,
}

impl Ledger {
    /// The ledger of the host about to start on `conn`, a `host.db`
    /// connection: the commands of the last `ledger_retention`. Blocking I/O:
    /// call it from startup.
    pub fn load(
        conn: &Connection,
        writer: Writer,
        clock: Arc<dyn Clock>,
        durations: &Durations,
    ) -> Result<Self, LedgerError> {
        let retention = durations.get(LEDGER_RETENTION);
        let table = row::load(conn, expired_up_to(clock.as_ref(), retention))?;
        Ok(Self {
            shared: Arc::new(Shared {
                table: Mutex::new(table.into_iter().collect()),
                writer,
                clock,
                retention,
                sweep_every: durations.get(LEDGER_SWEEP),
            }),
        })
    }

    /// Runs the command `principal` sent as `command` the first time, or
    /// answers the repeat. Only reads memory: the read loop calls it.
    pub fn begin(
        &self,
        principal: &PrincipalId,
        command: &CommandId,
        method: &str,
        params: &Value,
    ) -> Result<Begin, LedgerError> {
        let (key, hash) = (Key::new(principal, command), params_hash(params)?);
        let mut table = self.shared.lock();
        if let Some(repeat) = repeat(table.get(&key), method, &hash)? {
            return Ok(Begin::Repeat(repeat));
        }
        let begun = Begun {
            method: method.into(),
            hash,
            created_at: self.shared.clock.now_wall_ms(),
        };
        let (answer, outcome) = watch::channel(None);
        let state = State::Running(outcome);
        table.insert(
            key.clone(),
            Entry {
                begun: begun.clone(),
                state,
            },
        );
        drop(table);
        let shared = Arc::clone(&self.shared);
        Ok(Begin::Run(Ticket::new(shared, key, begun, answer)))
    }

    /// What a repeat of the command would get now, without running it:
    /// `None` when the ledger does not know the command.
    pub fn lookup(
        &self,
        principal: &PrincipalId,
        command: &CommandId,
        method: &str,
        params: &Value,
    ) -> Result<Option<Repeat>, LedgerError> {
        let (key, hash) = (Key::new(principal, command), params_hash(params)?);
        repeat(self.shared.lock().get(&key), method, &hash)
    }

    /// Drops the commands that began `ledger_retention` ago or earlier, from
    /// memory and from the table. A running command stays until it ends.
    /// Returns how many rows went.
    pub async fn sweep(&self) -> Result<usize, LedgerError> {
        let up_to = expired_up_to(self.shared.clock.as_ref(), self.shared.retention);
        self.shared.lock().retain(|_, entry| {
            entry.begun.created_at > up_to || matches!(entry.state, State::Running(_))
        });
        let receipt =
            (self.shared.writer).submit(Durability::Normal, move |conn| row::prune(conn, up_to));
        Ok(receipt.await?)
    }

    /// Sweeps at once and then every `ledger_sweep`, on `Clock` timers, for as
    /// long as the task runs. The host spawns it, as [`SWEEPER`].
    pub async fn sweeper(self, participant: Participant) {
        let clock = Arc::clone(&self.shared.clock);
        let mut next = clock.now_mono();
        loop {
            participant.idle(clock.timer_at(next)).await;
            // A failed sweep leaves its rows to the next one.
            let _ = self.sweep().await;
            next = next + self.shared.sweep_every;
        }
    }
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Entry>> {
        // Every update leaves the table consistent, so a poisoned lock is
        // still usable.
        self.table.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What a repeat of the command in `entry` gets, if the ledger knows it.
fn repeat(
    entry: Option<&Entry>,
    method: &str,
    hash: &Digest,
) -> Result<Option<Repeat>, LedgerError> {
    let Some(entry) = entry else {
        return Ok(None);
    };
    if *entry.begun.method != *method || entry.begun.hash != *hash {
        return Err(LedgerError::Mismatch);
    }
    match &entry.state {
        State::Running(outcome) => Ok(Some(Repeat::Wait(Waiter::new(outcome.clone())))),
        State::Done(outcome) => Ok(Some(Repeat::Replay(replayed(outcome.clone())))),
        State::Unknown => Err(LedgerError::OutcomeUnknown),
    }
}

/// A replayed result says so with `replayed: true`; an error stays as it was.
fn replayed(outcome: Outcome) -> Outcome {
    outcome.map(|mut result| {
        if let Value::Object(members) = &mut result {
            members.insert(REPLAYED.into(), Value::Bool(true));
        }
        result
    })
}

/// Commands that began at this wall-clock time or earlier have expired.
fn expired_up_to(clock: &dyn Clock, retention: Duration) -> u64 {
    let retention_ms = u64::try_from(retention.as_millis()).unwrap_or(u64::MAX);
    clock.now_wall_ms().saturating_sub(retention_ms)
}
