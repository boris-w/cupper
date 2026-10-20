//! Startup step 6's adapters (design 5.2.13, NFR-9): the process groups a
//! previous run's adapters left, each adapter's own and its descendants'
//! (`adapters.groups`, design 5.6.8), get SIGTERM, then SIGKILL what is left.
//! Without an ACP connection the first two steps of ADR-09 do not apply.

use std::time::Duration;

use cupper_sys::{ProcessGroup, Signal, StartTime};
use rusqlite::Connection;

use crate::clock::Clock;
use crate::driver::runtime::lifecycle;
use crate::durations::{Durations, REAP_POLL, REAP_TERM_WAIT};
use crate::store::{Durability, Writer};

/// The groups of the adapters that have not ended, as recorded.
fn leftovers(conn: &Connection) -> rusqlite::Result<Vec<ProcessGroup>> {
    let sql = "SELECT pgid, start_us, groups FROM adapters WHERE ended_at IS NULL";
    let mut rows = conn.prepare(sql)?;
    let rows = rows.query_map([], |row| {
        let own = (row.get::<_, i32>(0)?, row.get::<_, u64>(1)?);
        let groups: Option<String> = row.get(2)?;
        let more: Vec<(i32, u64)> = groups
            .and_then(|groups| serde_json::from_str(&groups).ok())
            .unwrap_or_default();
        Ok([vec![own], more].concat())
    })?;
    let groups = rows.collect::<rusqlite::Result<Vec<_>>>()?.concat();
    let groups = groups.into_iter().map(|(pgid, start)| ProcessGroup {
        pgid,
        start: StartTime::from_micros(start),
    });
    Ok(lifecycle::union(Vec::new(), &groups.collect::<Vec<_>>()))
}

/// Ends what the previous run's adapters left and marks their rows ended.
pub(super) async fn previous_run(
    conn: &Connection,
    writer: &Writer,
    clock: &dyn Clock,
    durations: &Durations,
) -> rusqlite::Result<()> {
    let groups = leftovers(conn)?;
    if groups.is_empty() {
        return Ok(());
    }
    let (wait, poll) = (durations.get(REAP_TERM_WAIT), durations.get(REAP_POLL));
    reap(&groups, clock, wait, poll).await;
    let at = clock.now_wall_ms();
    let ended = writer.submit(Durability::Normal, move |conn| {
        let sql =
            "UPDATE adapters SET ended_at = ?1, exit_status = 'reaped' WHERE ended_at IS NULL";
        conn.execute(sql, [at]).map(drop)
    });
    // Left unmarked, the next start signals the same recorded groups again.
    let _ = ended.await;
    Ok(())
}

/// SIGTERM to each group still the recorded one, then, polling every
/// `poll`, SIGKILL to what is left after `term_wait`.
pub async fn reap(groups: &[ProcessGroup], clock: &dyn Clock, term_wait: Duration, poll: Duration) {
    lifecycle::signal(groups, Signal::Term);
    let give_up = clock.now_mono() + term_wait;
    while groups.iter().any(ProcessGroup::is_alive) && clock.now_mono() < give_up {
        clock.timer_at(clock.now_mono() + poll).await;
    }
    lifecycle::signal(groups, Signal::Kill);
}
