//! The audit writer (design 5.8.6): each record the gateway, the sessions
//! and the index send goes to `host.log` and, in the form
//! [`AuditEvent::formal`](crate::gateway::AuditEvent::formal) gives it, into
//! the `audit` table, which keeps 90 days (`Cleanup::Audit`).
//! `stream.subscribed` goes to `host.log` only. Records hold no secrets: no
//! tokens, keys or login-shell environment reach them (design 5.8.5).

use tokio::sync::mpsc;

use crate::gateway::AuditRecord;
use crate::quiescence::{ActorKind, Participant};
use crate::store::{Durability, Writer};

/// The audit writer's kind in the quiescence barrier.
pub const AUDIT: ActorKind = ActorKind::new("audit");

/// Writes `records` until every sender is gone.
pub async fn write(
    mut records: mpsc::UnboundedReceiver<AuditRecord>,
    writer: Writer,
    participant: Participant,
) {
    while let Some(record) = participant.idle(records.recv()).await {
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
            continue;
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
}
