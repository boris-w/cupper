//! Retention (design 5.4.5): the cleanups the store runs on its own schedule,
//! timed through `Clock`, with durations from `durations.rs`.
//!
//! | Cleanup | Keeps | Runs |
//! |---|---|---|
//! | [`Cleanup::RawFrames`] | 14 days of `raw.db`, at most [`RAW_MAX_BYTES`] in use | at start, then hourly |
//! | [`Cleanup::Audit`] | 90 days of `audit` | at start, then daily |
//! | [`Cleanup::Adapters`] | running instances and the latest [`KEEP_ENDED_ADAPTERS`] ended | at start |
//! | [`Cleanup::OrphanBlobs`] | referenced blobs, and the rest for a day after their last put | at start |
//! | [`Cleanup::Backups`] | the latest [`KEEP_BACKUPS`] in `backups/` | at start, after the migration |
//!
//! The host exits when idle, so a process may not live for a day: the
//! periodic cleanups run at start as well. The ledger expires its own
//! commands (T032); the stream writers prune `events` (T026).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::Connection;
use tokio::task::JoinHandle;

use super::blobs::{BlobError, BlobStore, blocking, entries};
use super::connection::Durability;
use super::raw_writer::{RawRetention, RawWriter};
use super::writer::{WriteError, Writer};
use crate::clock::{Clock, MonoTime};
use crate::durations::{
    AUDIT_MAX_AGE, AUDIT_PERIOD, DurationKey, Durations, ORPHAN_BLOB_GRACE, RAW_FRAMES_MAX_AGE,
    RAW_FRAMES_PERIOD,
};
use crate::quiescence::{ActorKind, Quiescence};

/// The retention task's kind in the quiescence barrier. The host registers it
/// before [`Maintenance::spawn`].
pub const MAINTENANCE: ActorKind = ActorKind::new("store_maintenance");

/// Beyond this many bytes in use, `raw.db` loses its oldest frames.
pub const RAW_MAX_BYTES: u64 = 512 << 20;
/// Ended adapter instances kept in `adapters`.
pub const KEEP_ENDED_ADAPTERS: u32 = 100;
/// Backups kept in `backups/`.
pub const KEEP_BACKUPS: usize = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cleanup {
    RawFrames,
    Audit,
    /// Run it after the startup sweep has ended the instances a crash left
    /// running (T037), or they count as running.
    Adapters,
    OrphanBlobs,
    Backups,
}

impl Cleanup {
    pub const ALL: [Self; 5] = [
        Self::RawFrames,
        Self::Audit,
        Self::Adapters,
        Self::OrphanBlobs,
        Self::Backups,
    ];

    /// How often it runs after the round at start; `None` for only then.
    fn period(self) -> Option<DurationKey> {
        match self {
            Self::RawFrames => Some(RAW_FRAMES_PERIOD),
            Self::Audit => Some(AUDIT_PERIOD),
            Self::Adapters | Self::OrphanBlobs | Self::Backups => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MaintenanceError {
    #[error(transparent)]
    Write(#[from] WriteError),
    #[error(transparent)]
    Blob(#[from] BlobError),
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// When each cleanup is next due. Pure: it neither reads the clock nor does
/// I/O.
#[derive(Debug, Clone)]
pub struct Schedule {
    due: Vec<(Cleanup, MonoTime)>,
}

impl Schedule {
    /// Every cleanup due at `start`.
    pub fn starting(start: MonoTime) -> Self {
        Self {
            due: Cleanup::ALL.map(|cleanup| (cleanup, start)).to_vec(),
        }
    }

    pub fn next_deadline(&self) -> Option<MonoTime> {
        self.due.iter().map(|(_, at)| *at).min()
    }

    /// The cleanups due at `now`. A periodic one is due again a period after
    /// `now`, so a late timer does not bring on a burst of rounds.
    pub fn take_due(&mut self, now: MonoTime, durations: &Durations) -> Vec<Cleanup> {
        let mut due = Vec::new();
        self.due.retain_mut(|(cleanup, at)| {
            if *at > now {
                return true;
            }
            due.push(*cleanup);
            cleanup.period().is_some_and(|period| {
                *at = now + durations.get(period);
                true
            })
        });
        due
    }
}

/// Runs the cleanups.
#[derive(Debug)]
pub struct Maintenance {
    writer: Writer,
    raw: RawWriter,
    blobs: BlobStore,
    /// The data directory's `backups/`.
    backups: PathBuf,
    clock: Arc<dyn Clock>,
    durations: Durations,
}

impl Maintenance {
    pub fn new(
        writer: Writer,
        raw: RawWriter,
        blobs: BlobStore,
        backups: PathBuf,
        clock: Arc<dyn Clock>,
        durations: Durations,
    ) -> Self {
        Self {
            writer,
            raw,
            blobs,
            backups,
            clock,
            durations,
        }
    }

    /// Runs `cleanup` once, now. `test/run_maintenance` (T047) calls this.
    pub async fn run(&self, cleanup: Cleanup) -> Result<(), MaintenanceError> {
        let now_ms = self.clock.now_wall_ms();
        match cleanup {
            Cleanup::RawFrames => {
                let retention = RawRetention {
                    max_age: self.durations.get(RAW_FRAMES_MAX_AGE),
                    max_bytes: RAW_MAX_BYTES,
                };
                self.raw.prune(retention, now_ms).await?;
            }
            Cleanup::Audit => {
                let max_age = self.durations.get(AUDIT_MAX_AGE).as_millis();
                let cutoff = now_ms.saturating_sub(u64::try_from(max_age).unwrap_or(u64::MAX));
                let prune = move |conn: &Connection| {
                    conn.execute("DELETE FROM audit WHERE ts < ?1", [cutoff])
                };
                self.writer.submit(Durability::Normal, prune).await?;
            }
            Cleanup::Adapters => {
                let prune = |conn: &Connection| {
                    conn.execute(
                        "DELETE FROM adapters WHERE ended_at IS NOT NULL AND instance_id NOT IN
                           (SELECT instance_id FROM adapters WHERE ended_at IS NOT NULL
                            ORDER BY ended_at DESC LIMIT ?1)",
                        [KEEP_ENDED_ADAPTERS],
                    )
                };
                self.writer.submit(Durability::Normal, prune).await?;
            }
            Cleanup::OrphanBlobs => {
                self.blobs
                    .sweep(self.durations.get(ORPHAN_BLOB_GRACE))
                    .await?;
            }
            Cleanup::Backups => {
                let dir = self.backups.clone();
                blocking(move || prune_backups(&dir)).await?;
            }
        }
        Ok(())
    }

    /// Runs the cleanups `schedule` has due now, in turn.
    pub async fn tick(
        &self,
        schedule: &mut Schedule,
    ) -> Vec<(Cleanup, Result<(), MaintenanceError>)> {
        let due = schedule.take_due(self.clock.now_mono(), &self.durations);
        let mut ran = Vec::with_capacity(due.len());
        for cleanup in due {
            ran.push((cleanup, self.run(cleanup).await));
        }
        ran
    }

    /// Starts the retention task: every cleanup now, then the periodic ones
    /// on their periods, for as long as the task runs. It takes part in the
    /// quiescence barrier as [`MAINTENANCE`].
    pub fn spawn(self, quiescence: &Quiescence) -> JoinHandle<()> {
        let participant = quiescence.enter(MAINTENANCE);
        tokio::spawn(async move {
            let mut schedule = Schedule::starting(self.clock.now_mono());
            loop {
                // A cleanup that failed runs again on its next round; the host
                // has no log to report it to yet.
                let _ = self.tick(&mut schedule).await;
                let Some(next) = schedule.next_deadline() else {
                    return;
                };
                participant.idle(self.clock.timer_at(next)).await;
            }
        })
    }
}

/// Removes all but the latest [`KEEP_BACKUPS`] backups in `dir`, named
/// `host-v<version>-<YYYYMMDDTHHMMSSZ>.db`. The time decides: versions do not
/// sort as text (`v10` before `v9`). Other files, `host.partial` among them,
/// stay.
fn prune_backups(dir: &Path) -> io::Result<()> {
    let mut backups: Vec<(String, PathBuf)> = entries(dir)?
        .into_iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            let (_, time) = name
                .strip_prefix("host-v")?
                .strip_suffix(".db")?
                .rsplit_once('-')?;
            Some((time.to_owned(), path))
        })
        .collect();
    backups.sort();
    let stale = backups.len().saturating_sub(KEEP_BACKUPS);
    for (_, path) in &backups[..stale] {
        fs::remove_file(path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use cupper_protocol::ids::SessionId;

    use super::*;
    use crate::clock::ManualClock;
    use crate::store::blobs::add_ref;
    use crate::store::testutil::{
        TempDir, host_writer, query, raw_db, raw_frame, raw_rows, runtime,
    };
    use crate::store::{RAW_WRITER, RawWriterThread, WriterThread};

    const DAY_MS: u64 = 24 * 3600 * 1000;

    struct Fixture {
        dir: TempDir,
        clock: Arc<ManualClock>,
        maintenance: Maintenance,
        _threads: (WriterThread, RawWriterThread),
    }

    fn start(name: &str) -> Fixture {
        let dir = TempDir::new(name);
        let clock = Arc::new(ManualClock::new());
        let (writer, host) = host_writer(&dir, &clock);
        let quiescence = Quiescence::new();
        quiescence.register(RAW_WRITER);
        let (raw, raw_thread) = RawWriter::spawn(raw_db(&dir), &quiescence).unwrap();
        let blobs = BlobStore::new(dir.path(), writer.clone(), clock.clone());
        let backups = dir.path().join("backups");
        let durations = Durations::default();
        let maintenance = Maintenance::new(writer, raw, blobs, backups, clock.clone(), durations);
        Fixture {
            dir,
            clock,
            maintenance,
            _threads: (host, raw_thread),
        }
    }

    impl Fixture {
        fn now(&self) -> u64 {
            self.clock.now_wall_ms()
        }

        fn run(&self, cleanup: Cleanup) {
            runtime().block_on(self.maintenance.run(cleanup)).unwrap();
        }

        fn seed(&self, sql: String) {
            let seed = self
                .maintenance
                .writer
                .submit(Durability::Normal, move |conn| conn.execute_batch(&sql));
            runtime().block_on(seed).unwrap();
        }

        fn column(&self, sql: &str) -> Vec<u64> {
            query(&self.dir.host_db(), sql, |row| row.get(0))
        }

        fn raw_seqs(&self) -> Vec<u64> {
            raw_rows(&self.dir).iter().map(|row| row.seq).collect()
        }
    }

    #[test]
    fn unit_retention_raw_frames_keep_14_days() {
        let fx = start("retain-raw-age");
        let edge = fx.now() - 14 * DAY_MS;
        let sink = fx.maintenance.raw.sink();
        for (seq, ts) in [(1, edge - 1), (2, edge), (3, fx.now())] {
            sink.push(raw_frame(seq, ts, "{}"));
        }
        fx.run(Cleanup::RawFrames);
        assert_eq!(fx.raw_seqs(), [2, 3]);
        fx.clock.advance(Duration::from_millis(1));
        fx.run(Cleanup::RawFrames);
        assert_eq!(fx.raw_seqs(), [3]);
    }

    #[test]
    fn unit_retention_raw_frames_over_the_size_cap_go_oldest_first() {
        let fx = start("retain-raw-size");
        let sink = fx.maintenance.raw.sink();
        for seq in 1..=400 {
            sink.push(raw_frame(seq, seq, &"x".repeat(1000)));
        }
        let retention = RawRetention {
            max_age: Duration::MAX,
            max_bytes: 100 << 10,
        };
        let pruned = runtime().block_on(fx.maintenance.raw.prune(retention, fx.now()));
        let pruned = pruned.unwrap();
        let seqs = fx.raw_seqs();
        assert!(
            pruned.vacuumed && pruned.deleted + seqs.len() == 400,
            "{pruned:?}"
        );
        // The newest frames stay, and the file shrank to fit.
        assert_eq!(
            seqs,
            ((400 - seqs.len() as u64 + 1)..=400).collect::<Vec<_>>()
        );
        let raw = fx.dir.path().join("raw.db");
        let pragma = |name| query(&raw, &format!("PRAGMA {name}"), |row| row.get::<_, u64>(0))[0];
        assert!(pragma("page_count") * pragma("page_size") <= retention.max_bytes);
    }

    #[test]
    fn unit_retention_audit_keeps_90_days() {
        let fx = start("retain-audit");
        let edge = fx.now() - 90 * DAY_MS;
        fx.seed(format!(
            "INSERT INTO audit (ts, event) VALUES ({}, 'a'), ({edge}, 'a'), ({}, 'a')",
            edge - 1,
            fx.now()
        ));
        fx.run(Cleanup::Audit);
        assert_eq!(
            fx.column("SELECT ts FROM audit ORDER BY ts"),
            [edge, fx.now()]
        );
    }

    #[test]
    fn unit_retention_adapters_keep_the_running_and_the_latest_100_ended() {
        let fx = start("retain-adapters");
        let rows: Vec<String> = (1..=105)
            .map(|n| {
                let ended = if n > 103 {
                    "NULL".to_owned()
                } else {
                    n.to_string()
                };
                format!("('a_{n}', 'claude', 1, 1, 1, '1', '1', 0, {ended})")
            })
            .collect();
        fx.seed(format!(
            "INSERT INTO adapters (instance_id, agent, pid, pgid, start_us, adapter_version,
               node_version, started_at, ended_at) VALUES {}",
            rows.join(", ")
        ));
        fx.run(Cleanup::Adapters);
        let ended = fx.column("SELECT ended_at FROM adapters WHERE ended_at IS NOT NULL");
        assert_eq!(ended, (4..=103).collect::<Vec<_>>());
        let running = fx.column("SELECT count(*) FROM adapters WHERE ended_at IS NULL");
        assert_eq!(running, [2]);
    }

    #[test]
    fn unit_retention_orphan_blobs_go_a_day_after_their_last_put_unless_referenced() {
        let fx = start("retain-blobs");
        let blobs = &fx.maintenance.blobs;
        let put = |data: &str| {
            let put = blobs.put(data.as_bytes().to_vec(), "text/plain".to_owned(), None);
            runtime().block_on(put).unwrap().hash
        };
        let (kept, orphan) = (put("referenced"), put("orphan"));
        let session: SessionId = "s_1".parse().unwrap();
        let reference = fx.maintenance.writer.submit(Durability::Normal, {
            let (kept, session) = (kept.clone(), session.clone());
            move |conn| add_ref(conn, &kept, &session)
        });
        assert!(runtime().block_on(reference).unwrap());
        fx.clock.advance(Duration::from_millis(DAY_MS + 1));
        let young = put("young");
        // What crashes leave: a file without a row, and a temporary file.
        let stray = fx.dir.path().join("blobs/sha256/00");
        fs::create_dir(&stray).unwrap();
        let stray = stray.join("0".repeat(64));
        fs::write(&stray, b"stray").unwrap();
        fs::write(fx.dir.path().join("blobs/tmp/partial"), b"par").unwrap();

        fx.run(Cleanup::OrphanBlobs);
        let get = |hash| runtime().block_on(blobs.get(hash));
        assert_eq!(get(&kept).unwrap(), b"referenced");
        assert_eq!(get(&young).unwrap(), b"young");
        assert!(matches!(get(&orphan), Err(BlobError::NotFound)));
        assert!(!stray.exists());
        assert!(
            entries(&fx.dir.path().join("blobs/tmp"))
                .unwrap()
                .is_empty()
        );
        let rows = query(&fx.dir.host_db(), "SELECT hash FROM blobs", |row| {
            row.get(0)
        });
        assert!(rows.len() == 2 && !rows.contains(&orphan.to_string()));
        // A reference to a swept blob is refused: the content must come again.
        let late = fx
            .maintenance
            .writer
            .submit(Durability::Normal, move |conn| {
                add_ref(conn, &orphan, &session)
            });
        assert!(!runtime().block_on(late).unwrap());
    }

    #[test]
    fn unit_retention_backups_keep_the_latest_3() {
        let fx = start("retain-backups");
        let dir = fx.dir.path().join("backups");
        fs::create_dir_all(&dir).unwrap();
        let names = [
            "host-v9-20260101T000000Z.db",
            "host-v9-20260102T000000Z.db",
            "host-v10-20260103T000000Z.db",
            "host-v11-20260104T000000Z.db",
            "host.partial",
            "notes.txt",
        ];
        for name in names {
            fs::write(dir.join(name), b"").unwrap();
        }
        fx.run(Cleanup::Backups);
        let mut left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, [names[2], names[3], names[1], names[4], names[5]]);
    }

    #[test]
    fn unit_retention_schedule_runs_at_start_then_on_clock_periods() {
        let fx = start("retain-schedule");
        let hour = Duration::from_secs(3600);
        let start = fx.clock.now_mono();
        let mut schedule = Schedule::starting(start);
        let mut tick = || -> Vec<Cleanup> {
            let ran = runtime().block_on(fx.maintenance.tick(&mut schedule));
            ran.into_iter()
                .map(|(cleanup, done)| done.map(|()| cleanup).unwrap())
                .collect()
        };
        assert_eq!(tick(), Cleanup::ALL);
        fx.clock.advance(hour - Duration::from_millis(1));
        assert_eq!(tick(), []);
        fx.clock.advance(Duration::from_millis(1));
        assert_eq!(tick(), [Cleanup::RawFrames]);
        // A late round runs once and counts its period from when it ran.
        fx.clock.advance_to(start + hour * 24);
        assert_eq!(tick(), [Cleanup::RawFrames, Cleanup::Audit]);
        assert_eq!(schedule.next_deadline(), Some(start + hour * 25));
    }
}
