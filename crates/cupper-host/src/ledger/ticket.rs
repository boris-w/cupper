//! The first run of a command, from [`Ledger::begin`](super::Ledger::begin)
//! to its outcome, and the repeats that wait for it.

use std::sync::Arc;

use cupper_protocol::error::{ErrorCode, RpcError};
use rusqlite::Connection;
use tokio::sync::watch;

use super::{Begun, Key, LedgerError, Outcome, Shared, State, replayed, row};
use crate::store::Durability;

/// The first run of a command. End it with [`complete`](Self::complete) or
/// [`reject`](Self::reject); dropping it ends the run without an outcome:
/// waiting repeats get `internal`, and a retry runs the command again, or gets
/// `conflict/outcome_unknown` after [`mark_external`](Self::mark_external).
#[derive(Debug)]
#[must_use = "dropping a ticket ends the command without an outcome"]
pub struct Ticket {
    shared: Arc<Shared>,
    key: Key,
    begun: Begun,
    answer: watch::Sender<Option<Outcome>>,
    /// The `pending_external` row may be on disk.
    external: bool,
    ended: bool,
}

impl Ticket {
    pub(super) fn new(
        shared: Arc<Shared>,
        key: Key,
        begun: Begun,
        answer: watch::Sender<Option<Outcome>>,
    ) -> Self {
        Self {
            shared,
            key,
            begun,
            answer,
            external: false,
            ended: false,
        }
    }

    /// Records `pending_external` and returns once it is on disk
    /// (`synchronous=FULL` with `fullfsync`, design 5.2.8). A command that
    /// changes the user's repository (`git/commit`, `git/stage`, ...) calls it
    /// before it runs git: if the host stops before the outcome is recorded,
    /// a retry gets `conflict/outcome_unknown` instead of running git again.
    pub async fn mark_external(&mut self) -> Result<(), LedgerError> {
        let (key, begun) = (self.key.clone(), self.begun.clone());
        self.external = true;
        let written = self
            .shared
            .writer
            .submit(Durability::Full, move |conn| {
                row::upsert(conn, &key, &begun, None)
            })
            .await;
        // An error means nothing was committed.
        self.external = written.is_ok();
        Ok(written?)
    }

    /// The command ran and ended with `outcome`; the returned completion's
    /// row records it.
    pub fn complete(self, outcome: Outcome) -> Completion {
        Completion {
            json: row::outcome_json(&outcome).into(),
            outcome,
            ticket: self,
        }
    }

    /// The command failed before it ran (bad parameters, a precondition):
    /// nothing is recorded, so a retry runs it again. Repeats waiting on this
    /// run get `error` too.
    pub fn reject(mut self, error: RpcError) {
        if self.external {
            let key = self.key.clone();
            // If the host stops before this commits, the row stays, and a
            // retry gets `outcome_unknown`: the cautious answer.
            drop(
                (self.shared.writer)
                    .submit(Durability::Normal, move |conn| row::delete(conn, &key)),
            );
        }
        self.end(None);
        self.answer.send_replace(Some(Err(error)));
    }

    /// Moves the command out of the running state: to `state`, or out of
    /// the ledger.
    fn end(&mut self, state: Option<State>) {
        self.ended = true;
        let mut table = self.shared.lock();
        match state {
            Some(state) => {
                if let Some(entry) = table.get_mut(&self.key) {
                    entry.state = state;
                }
            }
            None => {
                table.remove(&self.key);
            }
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if !self.ended {
            self.end(self.external.then_some(State::Unknown));
        }
    }
}

/// A command's outcome on its way to disk, from [`Ticket::complete`].
/// Dropping it before [`committed`](Self::committed) is dropping the ticket.
#[derive(Debug)]
#[must_use = "a command counts as done only once its row is committed"]
pub struct Completion {
    ticket: Ticket,
    outcome: Outcome,
    json: Arc<str>,
}

impl Completion {
    /// The write that records the command. It goes into `Batch::with` of the
    /// batch of the command's first events, so that both commit in one
    /// transaction or neither does (design 5.2.3, rule 3).
    pub fn row(&self) -> impl FnOnce(&Connection) -> rusqlite::Result<()> + Send + 'static {
        let (key, begun) = (self.ticket.key.clone(), self.ticket.begun.clone());
        let json = Arc::clone(&self.json);
        move |conn: &Connection| row::upsert(conn, &key, &begun, Some(&json))
    }

    /// The row is committed: repeats get the outcome from now on, and those
    /// waiting get it now.
    pub fn committed(self) {
        let Self {
            mut ticket,
            outcome,
            ..
        } = self;
        ticket.end(Some(State::Done(outcome.clone())));
        ticket.answer.send_replace(Some(outcome));
    }

    /// Commits the row in a transaction of its own, for a command without
    /// events.
    pub async fn commit(self) -> Result<(), LedgerError> {
        let receipt = (self.ticket.shared.writer).submit(Durability::Normal, self.row());
        receipt.await?;
        self.committed();
        Ok(())
    }
}

/// A repeat that waits for the first run of its command.
#[derive(Debug)]
pub struct Waiter(watch::Receiver<Option<Outcome>>);

impl Waiter {
    pub(super) fn new(outcome: watch::Receiver<Option<Outcome>>) -> Self {
        Self(outcome)
    }

    /// The first run's outcome, as a replay; `internal` if the run ended
    /// without one.
    pub async fn outcome(mut self) -> Outcome {
        match self.0.wait_for(Option::is_some).await {
            Ok(outcome) => replayed(outcome.clone().expect("waited for an outcome")),
            Err(_) => Err(RpcError::app(
                ErrorCode::Internal,
                "the command's first run ended without an outcome",
            )),
        }
    }
}
