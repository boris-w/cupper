//! Startup steps 5 and 6 on `host.db` (design 5.2.13), in one transaction
//! before the writer takes the connection: the `data_epoch` decision and the
//! `meta` of this run (design 5.2.8), then what the previous run left running
//! ends.
//!
//! Only the rows change (ruled before T037): running turns end as
//! `interrupted`, pending interactions as `expired` with `host_restarted`,
//! and running sessions become `interrupted`. The session actor (T043) writes
//! the matching events when it loads the session, as the only writer of its
//! stream. The leftover adapters are T059's.

use cupper_protocol::ids::HostId;
use rusqlite::{Connection, TransactionBehavior};

use super::StartError;
use crate::store::epoch::{self, Boot, BootSession, EpochStep, LastExit};
use crate::store::meta::{self, MetaKey};

/// What this run goes by.
#[derive(Debug)]
pub(super) struct Run {
    pub host_id: HostId,
    pub epoch: u64,
    /// The previous run did not exit in order (`host.updated`).
    pub abnormal: bool,
}

pub(super) fn begin_run(
    conn: &mut Connection,
    boot: BootSession,
    now_ms: u64,
) -> Result<Run, StartError> {
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let read = |key| meta::get(&tx, key);
    let number = |key| match read(key)? {
        None => Ok(None),
        Some(text) => text.parse().map(Some).map_err(|_| StartError::Meta(key)),
    };
    let last_exit = LastExit::from_meta(
        number(MetaKey::LastStartAt)?,
        number(MetaKey::LastCleanExitAt)?,
    );
    let previous_boot = read(MetaKey::BootSessionUuid)?;
    let mut epoch = number(MetaKey::DataEpoch)?.ok_or(StartError::Meta(MetaKey::DataEpoch))?;
    if epoch::decide(last_exit, Boot::since(previous_boot.as_deref(), boot)) == EpochStep::Bump {
        epoch += 1;
    }
    let host_id = read(MetaKey::HostId)?
        .and_then(|id| id.parse().ok())
        .ok_or(StartError::Meta(MetaKey::HostId))?;
    meta::set(&tx, MetaKey::LastStartAt, &now_ms.to_string())?;
    meta::set(&tx, MetaKey::BootSessionUuid, &boot.to_string())?;
    meta::set(&tx, MetaKey::DataEpoch, &epoch.to_string())?;
    tx.execute(
        "UPDATE turns SET status = 'interrupted', ended_at = ?1 WHERE status = 'running'",
        [now_ms],
    )?;
    tx.execute(
        "UPDATE interactions SET status = 'expired', reason = 'host_restarted', resolved_at = ?1
         WHERE status = 'pending'",
        [now_ms],
    )?;
    tx.execute(
        "UPDATE sessions SET state = 'interrupted', updated_at = ?1 WHERE state = 'running'",
        [now_ms],
    )?;
    tx.commit()?;
    let abnormal = last_exit == LastExit::Unclean;
    Ok(Run {
        host_id,
        epoch,
        abnormal,
    })
}
