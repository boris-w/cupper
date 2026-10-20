//! The audit writer (design 5.8.6): each record the gateway, the sessions
//! and the index send goes to `host.log` and, in the form
//! [`AuditEvent::formal`](crate::gateway::AuditEvent::formal) gives it, into
//! the `audit` table, which keeps 90 days (`Cleanup::Audit`).
//! `stream.subscribed` goes to `host.log` only. Records hold no secrets: no
//! tokens, keys or login-shell environment reach them (design 5.8.5).

use tokio::sync::{mpsc, oneshot};

use crate::gateway::{AuditEvent, AuditRecord};
use crate::quiescence::{ActorKind, Participant};
use crate::store::{Durability, Writer};

/// The audit writer's kind in the quiescence barrier.
pub const AUDIT: ActorKind = ActorKind::new("audit");

/// Asks the audit writer to store what came before; the orderly exit does,
/// before the store writer stops.
#[derive(Debug)]
pub struct Drain(pub mpsc::UnboundedSender<oneshot::Sender<()>>);

impl Drain {
    /// Returns once every record sent before the call is stored.
    pub async fn wait(&self) {
        let (done, stored) = oneshot::channel();
        if self.0.send(done).is_ok() {
            let _ = stored.await;
        }
    }
}

/// Writes `records` until every sender is gone, answering `drains` once
/// none is waiting.
pub async fn write(
    mut records: mpsc::UnboundedReceiver<AuditRecord>,
    mut drains: mpsc::UnboundedReceiver<oneshot::Sender<()>>,
    writer: Writer,
    participant: Participant,
) {
    loop {
        let next = participant.idle(async {
            tokio::select! {
                biased;
                record = records.recv() => Ok(record),
                Some(done) = drains.recv() => Err(done),
            }
        });
        match next.await {
            Ok(Some(record)) => store(record, &writer).await,
            Ok(None) => break,
            Err(done) => drop(done.send(())),
        }
    }
}

async fn store(mut record: AuditRecord, writer: &Writer) {
    if let AuditEvent::Accepted { .. } = record.event {
        let read = tokio::task::spawn_blocking(move || {
            record.event.read_peer();
            record
        });
        let Ok(read) = read.await else {
            return;
        };
        record = read;
    }
    let formal = record.event.formal();
    let conn = record.conn.map(|conn| conn.to_string());
    let principal = formal.principal.map(|principal| principal.to_string());
    let peer = formal.peer.map(|peer| peer.to_string());
    let detail = formal.detail.to_string();
    tracing::info!(
        event = %formal.event,
        conn = conn.as_deref(),
        principal = principal.as_deref(),
        peer = peer.as_deref(),
        detail = %detail,
        "audit"
    );
    if !record.event.kept() {
        return;
    }
    let row = (record.at_ms, formal.event, conn, principal, peer, detail);
    let written = writer.submit(Durability::Normal, move |db| {
        db.execute(
            "INSERT INTO audit (ts, event, conn_id, principal, peer_json, detail_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![row.0, row.1, row.2, row.3, row.4, row.5],
        )
    });
    if let Err(err) = written.await {
        tracing::warn!(%err, "an audit record was not stored");
    }
}

#[cfg(test)]
mod tests {
    use cupper_protocol::ids::SessionId;
    use cupper_protocol::session::ConfirmationReason;

    use super::*;
    use crate::clock::ManualClock;
    use crate::gateway::{AuditEvent, ConfigState, HostEvent};
    use crate::quiescence::Quiescence;
    use crate::store::testutil::{TempDir, host_writer, query};

    #[tokio::test]
    async fn unit_audit_format_a_session_s_record_is_stored_without_a_connection() {
        let (dir, clock) = (TempDir::new("audit"), ManualClock::new());
        let (writer, _thread) = host_writer(&dir, &clock);
        let quiescence = Quiescence::new();
        quiescence.register(AUDIT);
        let (records, audit) = mpsc::unbounded_channel();
        let (_drain, drains) = mpsc::unbounded_channel();
        let written = tokio::spawn(write(audit, drains, writer, quiescence.enter(AUDIT)));
        let config = ConfigState {
            session_id: SessionId::from_uuid(1),
            mode: None,
            tier: None,
            set_by: "pr_cli".parse().unwrap(),
        };
        let reason = ConfirmationReason::FullAccessUnconfirmed;
        let event = AuditEvent::ConfigUnconfirmed { reason, config };
        let (at_ms, conn) = (1_000, None);
        records.send(AuditRecord { at_ms, conn, event }).unwrap();
        drop(records);
        written.await.unwrap();
        let sql = "SELECT ts || ' ' || event || ' ' || coalesce(conn_id, '-') || ' ' || principal
                   FROM audit";
        let rows: Vec<String> = query(&dir.host_db(), sql, |row| row.get(0));
        assert_eq!(rows, ["1000 config.unconfirmed - pr_cli"]);
    }

    /// The orderly exit's drain: what was sent before it is stored by then,
    /// with the senders still alive.
    #[tokio::test]
    async fn unit_audit_producers_a_drain_returns_once_what_came_before_is_stored() {
        let (dir, clock) = (TempDir::new("drain"), ManualClock::new());
        let (writer, _thread) = host_writer(&dir, &clock);
        let quiescence = Quiescence::new();
        quiescence.register(AUDIT);
        let (records, audit) = mpsc::unbounded_channel();
        let (drain, drains) = mpsc::unbounded_channel();
        tokio::spawn(write(audit, drains, writer, quiescence.enter(AUDIT)));
        for n in 0..3 {
            let event = AuditEvent::Host(HostEvent::PendingUpgrade);
            records
                .send(AuditRecord {
                    at_ms: n,
                    conn: None,
                    event,
                })
                .unwrap();
        }
        Drain(drain).wait().await;
        let rows: Vec<u64> = query(&dir.host_db(), "SELECT ts FROM audit", |row| row.get(0));
        assert_eq!(rows, [0, 1, 2]);
    }
}
