//! UNIT-ledger, on a real `host.db` and writer thread.

use std::time::Duration;

use cupper_protocol::error::{ErrorCode, ErrorReason};
use cupper_protocol::event::{EventBody, Strictness};
use cupper_protocol::sync::StreamId;
use rusqlite::params;
use serde_json::json;
use tokio::runtime::Runtime;

use super::*;
use crate::clock::{ManualClock, MonoTime};
use crate::gateway::testkit::TempDir;
use crate::quiescence::Quiescence;
use crate::store::streams::{Batch, Streams, StreamsConfig};
use crate::store::{
    Database, Durability, MIGRATIONS, Readers, WRITER, WriterConfig, migrate, open_writer,
};

const METHOD: &str = "session/send";
const HOUR: Duration = Duration::from_secs(60 * 60);

struct Fixture {
    _dir: TempDir,
    clock: ManualClock,
    writer: Writer,
    readers: Readers,
    runtime: Runtime,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = TempDir::new(name);
        let (host_db, clock) = (dir.path().join("host.db"), ManualClock::new());
        let mut conn = open_writer(&host_db, Database::Host).unwrap();
        migrate(&mut conn, MIGRATIONS, &dir.path().join("backups"), &clock).unwrap();
        let quiescence = Quiescence::new();
        quiescence.register(WRITER);
        let config = WriterConfig {
            linger: Duration::ZERO,
        };
        let (writer, _) = Writer::spawn(conn, config, &quiescence).unwrap();
        let readers = Readers::open(&host_db).unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        Self {
            _dir: dir,
            clock,
            writer,
            readers,
            runtime,
        }
    }

    /// A ledger loaded from the table, as the host loads it when it starts.
    fn start(&self) -> Ledger {
        self.load().unwrap()
    }

    fn load(&self) -> Result<Ledger, LedgerError> {
        let (writer, clock) = (self.writer.clone(), Arc::new(self.clock.clone()));
        let load = |conn: &Connection| Ok(Ledger::load(conn, writer, clock, &Durations::default()));
        self.runtime.block_on(self.readers.read(load)).unwrap()
    }

    fn commit(&self, ticket: Ticket, outcome: Outcome) {
        let completion = ticket.complete(outcome);
        self.runtime.block_on(completion.commit()).unwrap();
    }

    /// `(principal, command_id, state)` of every row, after the writes
    /// submitted so far.
    fn rows(&self) -> Vec<(String, String, String)> {
        let read = self.writer.submit(Durability::Normal, |conn| {
            conn.prepare("SELECT principal, command_id, state FROM commands ORDER BY 1, 2")?
                .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
                .collect()
        });
        self.runtime.block_on(read).unwrap()
    }

    fn count_events(&self) -> u64 {
        let read = self
            .readers
            .read(|conn| conn.query_row("SELECT count(*) FROM events", [], |row| row.get(0)));
        self.runtime.block_on(read).unwrap()
    }

    /// Waits until the sweeper has swept and set a timer later than `after`.
    fn swept(&self, after: MonoTime) {
        self.runtime.block_on(async {
            while self.clock.next_deadline().is_none_or(|next| next <= after) {
                tokio::task::yield_now().await;
            }
        });
    }

    /// Moves the clock to the sweeper's next timer and waits for that sweep.
    fn next_sweep(&self) {
        let due = self.clock.next_deadline().expect("the sweeper waits");
        self.clock.advance_to(due);
        self.swept(due);
    }
}

fn row(principal: &str, command: &str, state: &str) -> (String, String, String) {
    (principal.into(), command.into(), state.into())
}

/// `begin` of `id` from `pr_desktop`.
fn begin(ledger: &Ledger, id: &str, params: &Value) -> Result<Begin, LedgerError> {
    let principal = "pr_desktop".parse().unwrap();
    ledger.begin(&principal, &id.parse().unwrap(), METHOD, params)
}

fn ticket(begun: Result<Begin, LedgerError>) -> Ticket {
    match begun {
        Ok(Begin::Run(ticket)) => ticket,
        other => panic!("expected a first run, got {other:?}"),
    }
}

fn replay(begun: Result<Begin, LedgerError>) -> Outcome {
    match begun {
        Ok(Begin::Repeat(Repeat::Replay(outcome))) => outcome,
        other => panic!("expected a replay, got {other:?}"),
    }
}

fn waiter(begun: Result<Begin, LedgerError>) -> Waiter {
    match begun {
        Ok(Begin::Repeat(Repeat::Wait(waiter))) => waiter,
        other => panic!("expected to wait, got {other:?}"),
    }
}

#[test]
fn unit_ledger_first_run_is_replayed_to_repeats_and_after_restart() {
    let fx = Fixture::new("ledger-replay");
    let ledger = fx.start();
    let params = json!({"session_id": "s_1", "content": [{"type": "text", "text": "hi"}]});
    let lookup = |ledger: &Ledger| {
        let principal = "pr_desktop".parse().unwrap();
        ledger.lookup(&principal, &"c1".parse().unwrap(), METHOD, &params)
    };
    // A lookup starts nothing.
    assert!(matches!(lookup(&ledger), Ok(None)));
    let first = ticket(begin(&ledger, "c1", &params));
    assert!(matches!(lookup(&ledger), Ok(Some(Repeat::Wait(_)))));

    fx.commit(first, Ok(json!({"turn_id": "t_1"})));
    let replayed = Ok(json!({"turn_id": "t_1", "replayed": true}));
    assert!(matches!(lookup(&ledger), Ok(Some(Repeat::Replay(o))) if o == replayed));
    assert_eq!(replay(begin(&ledger, "c1", &params)), replayed);
    // An error the command ended with is its outcome too.
    let hook = RpcError::app(ErrorCode::GitError, "hook").with_reason(ErrorReason::HookFailed);
    fx.commit(ticket(begin(&ledger, "c2", &params)), Err(hook.clone()));
    assert_eq!(replay(begin(&ledger, "c2", &params)), Err(hook.clone()));

    let restarted = fx.start();
    assert_eq!(replay(begin(&restarted, "c1", &params)), replayed);
    assert_eq!(replay(begin(&restarted, "c2", &params)), Err(hook));
    let done = [
        row("pr_desktop", "c1", "done"),
        row("pr_desktop", "c2", "done"),
    ];
    assert_eq!(fx.rows(), done);
}

#[test]
fn unit_ledger_other_params_or_method_are_a_mismatch() {
    let fx = Fixture::new("ledger-mismatch");
    let ledger = fx.start();
    let (params, other) = (json!({"session_id": "s_1"}), json!({"session_id": "s_2"}));
    fx.commit(ticket(begin(&ledger, "done", &params)), Ok(json!({})));
    let running = ticket(begin(&ledger, "running", &params));

    for id in ["done", "running"] {
        assert!(matches!(
            begin(&ledger, id, &other),
            Err(LedgerError::Mismatch)
        ));
        let principal = "pr_desktop".parse().unwrap();
        let interrupt = ledger.begin(
            &principal,
            &id.parse().unwrap(),
            "session/interrupt",
            &params,
        );
        assert!(matches!(interrupt, Err(LedgerError::Mismatch)), "{id}");
    }
    drop(running);
}

#[test]
fn unit_ledger_inflight_repeats_wait_for_the_first_run() {
    let fx = Fixture::new("ledger-inflight");
    let ledger = fx.start();
    let params = json!({"session_id": "s_1"});

    let first = ticket(begin(&ledger, "c1", &params));
    let waiting = fx
        .runtime
        .spawn(waiter(begin(&ledger, "c1", &params)).outcome());
    let later = waiter(begin(&ledger, "c1", &params));
    fx.commit(first, Ok(json!({"turn_id": "t_1"})));
    let replayed = Ok(json!({"turn_id": "t_1", "replayed": true}));
    assert_eq!(fx.runtime.block_on(waiting).unwrap(), replayed);
    assert_eq!(fx.runtime.block_on(later.outcome()), replayed);

    // A run that fails before it ran answers its waiters, records nothing,
    // and a retry runs again.
    let rejected = ticket(begin(&ledger, "c2", &params));
    let waiting = waiter(begin(&ledger, "c2", &params));
    let missing = RpcError::app(ErrorCode::NotFound, "no such session");
    rejected.reject(missing.clone());
    assert_eq!(fx.runtime.block_on(waiting.outcome()), Err(missing));
    // A run that ends without an outcome leaves its waiters `internal`.
    let dropped = ticket(begin(&ledger, "c2", &params));
    let waiting = waiter(begin(&ledger, "c2", &params));
    drop(dropped);
    let internal = fx.runtime.block_on(waiting.outcome()).unwrap_err();
    assert_eq!(internal.app_code(), Some(&ErrorCode::Internal));
    drop(ticket(begin(&ledger, "c2", &params)));
    assert_eq!(fx.rows(), [row("pr_desktop", "c1", "done")]);
}

#[test]
fn unit_ledger_restart_leaves_outcome_unknown_only_where_it_began() {
    let fx = Fixture::new("ledger-unknown");
    let ledger = fx.start();
    let params = json!({"session_id": "s_1", "message": "m"});
    fx.commit(
        ticket(begin(&ledger, "done", &params)),
        Ok(json!({"oid": "1"})),
    );
    let mut git = ticket(begin(&ledger, "git", &params));
    fx.runtime.block_on(git.mark_external()).unwrap();
    let running = ticket(begin(&ledger, "running", &params));
    let rows = [
        row("pr_desktop", "done", "done"),
        row("pr_desktop", "git", "pending_external"),
    ];
    assert_eq!(fx.rows(), rows);

    // The host stops here, with both runs going.
    let restarted = fx.start();
    assert_eq!(
        replay(begin(&restarted, "done", &params)),
        Ok(json!({"oid": "1", "replayed": true}))
    );
    assert!(matches!(
        begin(&restarted, "git", &params),
        Err(LedgerError::OutcomeUnknown)
    ));
    drop(ticket(begin(&restarted, "running", &params)));

    // Within one run: an external command whose run ends without an outcome
    // is unknown; one that completes is done; one rejected is forgotten.
    drop((git, running));
    assert!(matches!(
        begin(&ledger, "git", &params),
        Err(LedgerError::OutcomeUnknown)
    ));
    for (id, reject) in [("commit", false), ("stale", true)] {
        let mut ticket = ticket(begin(&restarted, id, &params));
        fx.runtime.block_on(ticket.mark_external()).unwrap();
        if reject {
            ticket.reject(
                RpcError::app(ErrorCode::Conflict, "stale").with_reason(ErrorReason::StaleDiff),
            );
        } else {
            fx.commit(ticket, Ok(json!({"oid": "2"})));
        }
    }
    drop(ticket(begin(&restarted, "stale", &params)));
    let rows = [
        row("pr_desktop", "commit", "done"),
        row("pr_desktop", "done", "done"),
        row("pr_desktop", "git", "pending_external"),
    ];
    assert_eq!(fx.rows(), rows);
}

#[test]
fn unit_ledger_load_refuses_a_bad_row() {
    let fx = Fixture::new("ledger-corrupt");
    let bad = [
        ("running", None),
        ("done", Some("{}")),
        ("pending_external", Some(r#"{"result": {}}"#)),
    ];
    for (state, result) in bad {
        let write = fx.writer.submit(Durability::Normal, move |conn| {
            conn.execute(
                "INSERT OR REPLACE INTO commands VALUES ('pr_cli', 'c1', 'm', 'd_0', ?1, ?2, ?3)",
                params![state, result, ManualClock::DEFAULT_START_WALL_MS],
            )
        });
        fx.runtime.block_on(write).unwrap();
        assert!(matches!(fx.load(), Err(LedgerError::Corrupt(_))), "{state}");
    }
}

#[test]
fn unit_ledger_commands_expire_after_48_hours_by_clock() {
    let fx = Fixture::new("ledger-expiry");
    let ledger = fx.start();
    let params = json!({});
    fx.commit(ticket(begin(&ledger, "early", &params)), Ok(json!({})));
    fx.clock.advance(48 * HOUR);
    // A host starting now does not load it, though the row is still there.
    drop(ticket(begin(&fx.start(), "early", &params)));
    assert_eq!(fx.rows(), [row("pr_desktop", "early", "done")]);

    // The sweeper sweeps at once, then every hour.
    let quiescence = Quiescence::new();
    quiescence.register(super::SWEEPER);
    fx.runtime
        .spawn(ledger.clone().sweeper(quiescence.enter(super::SWEEPER)));
    fx.swept(fx.clock.now_mono());
    drop(ticket(begin(&ledger, "early", &params)));
    assert!(fx.rows().is_empty());
    fx.commit(ticket(begin(&ledger, "late", &params)), Ok(json!({})));
    for _ in 0..47 {
        fx.next_sweep();
    }
    assert_eq!(
        replay(begin(&ledger, "late", &params)),
        Ok(json!({"replayed": true}))
    );
    fx.next_sweep();
    drop(ticket(begin(&ledger, "late", &params)));
    assert!(fx.rows().is_empty());
}

#[test]
fn unit_ledger_params_hash_ignores_command_id_attestation_input_and_key_order() {
    let base = json!({"session_id": "s_1", "outcome": {"option_id": "allow", "notes": [1, 2]}});
    let same = [
        json!({"outcome": {"notes": [1, 2], "option_id": "allow"}, "session_id": "s_1"}),
        json!({"session_id": "s_1", "outcome": {"option_id": "allow", "notes": [1, 2]},
            "command_id": "c9", "attestation": {"sig": "x"}, "input": {"kind": "click"}}),
    ];
    let other = [
        json!({"session_id": "s_1", "outcome": {"option_id": "deny", "notes": [1, 2]}}),
        json!({"session_id": "s_1", "outcome": {"option_id": "allow", "notes": [2, 1]}}),
        // Only the top-level fields are left out.
        json!({"session_id": "s_1", "outcome": {"option_id": "allow", "notes": [1, 2], "input": 1}}),
        json!({"session_id": "s_1", "outcome": {"option_id": "allow", "notes": [1, 2]}, "x": null}),
    ];
    let hash = params_hash(&base).unwrap();
    for params in &same {
        assert_eq!(params_hash(params).unwrap(), hash, "{params}");
    }
    for params in &other {
        assert_ne!(params_hash(params).unwrap(), hash, "{params}");
    }

    let fx = Fixture::new("ledger-hash");
    let ledger = fx.start();
    fx.commit(ticket(begin(&ledger, "c1", &base)), Ok(json!({})));
    for params in &same {
        assert_eq!(
            replay(begin(&ledger, "c1", params)),
            Ok(json!({"replayed": true}))
        );
    }
}

#[test]
fn unit_ledger_principals_keep_their_own_command_ids() {
    let fx = Fixture::new("ledger-principals");
    let ledger = fx.start();
    let command = "c1".parse().unwrap();
    for (principal, n) in [("pr_desktop", 1), ("pr_cli", 2)] {
        let begun = ledger.begin(
            &principal.parse().unwrap(),
            &command,
            METHOD,
            &json!({"n": n}),
        );
        fx.commit(ticket(begun), Ok(json!({"n": n})));
    }
    for (principal, n) in [("pr_desktop", 1), ("pr_cli", 2)] {
        let begun = ledger.begin(
            &principal.parse().unwrap(),
            &command,
            METHOD,
            &json!({"n": n}),
        );
        assert_eq!(replay(begun), Ok(json!({"n": n, "replayed": true})));
    }
    assert_eq!(
        fx.rows(),
        [row("pr_cli", "c1", "done"), row("pr_desktop", "c1", "done")]
    );
}

#[test]
fn unit_ledger_result_commits_with_its_first_events_or_not_at_all() {
    let fx = Fixture::new("ledger-with");
    let ledger = fx.start();
    let streams = Streams::new(
        fx.writer.clone(),
        fx.readers.clone(),
        Arc::new(fx.clock.clone()),
        StreamsConfig::new(1, &Durations::default()),
    );
    let handle = fx.runtime.block_on(streams.open(&StreamId::Index)).unwrap();
    let mut stream = handle.writer().unwrap();
    let project = |n: u32| {
        let raw = json!({"type": "project.upserted", "data": {"project_id": format!("p_{n}"),
            "name": "p", "path": "/w", "is_git": false, "added_at": n, "position": n,
            "trust_state": "trusted"}});
        EventBody::decode(&raw, Strictness::Strict).unwrap().value
    };
    let params = json!({"path": "/w"});

    let completion =
        ticket(begin(&ledger, "c1", &params)).complete(Ok(json!({"project_id": "p_1"})));
    let batch = Batch {
        bodies: vec![project(1)],
        with: Some(Box::new(completion.row())),
        ..Batch::default()
    };
    let appended = fx.runtime.block_on(stream.append(batch)).unwrap();
    assert!(appended.rejected.is_empty());
    completion.committed();
    assert_eq!(
        (fx.count_events(), fx.rows()),
        (1, vec![row("pr_desktop", "c1", "done")])
    );

    // The row fails in the transaction: the events go with it.
    let refuse = "CREATE TEMP TRIGGER refuse BEFORE INSERT ON main.commands
                  BEGIN SELECT RAISE(ABORT, 'refused'); END";
    let trigger = fx
        .writer
        .submit(Durability::Normal, move |conn| conn.execute_batch(refuse));
    fx.runtime.block_on(trigger).unwrap();
    let completion =
        ticket(begin(&ledger, "c2", &params)).complete(Ok(json!({"project_id": "p_2"})));
    let waiting = waiter(begin(&ledger, "c2", &params));
    let batch = Batch {
        bodies: vec![project(2)],
        with: Some(Box::new(completion.row())),
        ..Batch::default()
    };
    assert!(fx.runtime.block_on(stream.append(batch)).is_err());
    drop(completion);
    assert_eq!(
        (fx.count_events(), fx.rows()),
        (1, vec![row("pr_desktop", "c1", "done")])
    );
    let internal = fx.runtime.block_on(waiting.outcome()).unwrap_err();
    assert_eq!(internal.app_code(), Some(&ErrorCode::Internal));
    drop(ticket(begin(&ledger, "c2", &params)));
}

#[test]
fn unit_ledger_errors_map_to_protocol_codes() {
    let wire = |err: LedgerError| serde_json::to_value(RpcError::from(err)).unwrap();
    assert_eq!(
        wire(LedgerError::Mismatch)["data"],
        json!({"code": "idempotency_mismatch"})
    );
    assert_eq!(
        wire(LedgerError::OutcomeUnknown)["data"],
        json!({"code": "conflict", "reason": "outcome_unknown"})
    );
    let unsafe_integer = params_hash(&json!({"n": 1_u64 << 60})).unwrap_err();
    let invalid = wire(unsafe_integer.into());
    assert_eq!(invalid["code"], INVALID_PARAMS);
    assert!(invalid.get("data").is_none());
}
