//! The host's PERF cases (design 5.9.6), nightly: PERF-host-cold-start,
//! PERF-index-snapshot and PERF-host-memory. `cargo xtask ci perf` runs them
//! on a release build, one at a time, and keeps their samples
//! (`cupper_host::perf::record`); budgets are judged on the reference
//! machine only, so the tests assert none. The data: 200 sessions from
//! `test/seed`, one with a turn of 1,000 items; the agent is the fake agent.

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use cupper_cli::client::{Client, ClientAuth, Incoming, Options};
use cupper_cli::harness::HarnessKey;
use cupper_host::clock::{Clock, MonoTime, SystemClock};
use cupper_host::gateway::testkit::{KEY, TempDir};
use cupper_host::perf::record;
use cupper_protocol::methods::admin::TestSeed;
use cupper_protocol::methods::stream::StreamSubscribeParams;
use cupper_protocol::sync::StreamId;
use serde_json::json;

use common::*;

/// Samples per metric (design 5.9.6: at least 30).
const SAMPLES: usize = 30;

fn ms(from: MonoTime, to: MonoTime) -> f64 {
    to.saturating_duration_since(from).as_secs_f64() * 1000.0
}

fn harness() -> Options {
    Options {
        auth: ClientAuth::Harness(HarnessKey::new(KEY)),
        ..Options::default()
    }
}

/// The harness client as soon as the host listens: tried every millisecond.
async fn connect_soon(data_dir: &Path) -> Client {
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    loop {
        if let Ok(client) = connect_once(data_dir, harness()).await {
            return client;
        }
        assert!(clock.now_mono() < give_up, "the host does not listen");
        clock
            .timer_at(clock.now_mono() + Duration::from_millis(1))
            .await;
    }
}

/// Subscribes to the index and waits for its snapshot's last page; the
/// sessions in it.
async fn index_snapshot(client: &mut Client) -> usize {
    let index = StreamSubscribeParams {
        stream: StreamId::Index,
        after_seq: None,
        epoch: None,
    };
    client.subscribe(index).await.unwrap();
    let clock = SystemClock::new();
    let mut give_up = clock.timer_at(clock.now_mono() + PATIENCE);
    let mut sessions = 0;
    loop {
        let incoming = tokio::select! {
            incoming = client.next() => incoming.expect("the connection is open"),
            () = &mut give_up => panic!("no index snapshot"),
        };
        if let Incoming::Snapshot(page) = incoming {
            let upserted = page
                .ops
                .iter()
                .filter(|op| op.as_value()["type"] == "session.upserted");
            sessions += upserted.count();
            if page.last {
                return sessions;
            }
        }
    }
}

/// A data directory of 200 seeded sessions in one project; the host stopped.
async fn seeded(dir: &TempDir) -> PathBuf {
    let data_dir = fake_agent_data(dir);
    let mut host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let client = connect(&data_dir).await;
    let profile = json!({"profile": {"sessions": 200, "turns": 5, "items_per_turn": 20,
        "mix": "sp1", "large_turn_items": 1000}});
    client.request::<TestSeed>(typed(profile)).await.unwrap();
    terminate(&host.0);
    assert_eq!(exit_code(&mut host.0).await, 0);
    data_dir
}

/// PERF-host-cold-start (ADR-01): from starting the host process to a
/// client holding the whole index snapshot. The system's file cache is not
/// emptied (that takes an administrator).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "PERF-host-cold-start runs nightly"]
async fn perf_host_cold_start_to_the_index_snapshot() {
    let dir = TempDir::new("perf-cold");
    let data_dir = seeded(&dir).await;
    let clock = SystemClock::new();
    let mut samples = Vec::new();
    for _ in 0..SAMPLES {
        let start = clock.now_mono();
        let mut host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
        let mut client = connect_soon(&data_dir).await;
        assert_eq!(index_snapshot(&mut client).await, 200);
        samples.push(ms(start, clock.now_mono()));
        drop(client);
        terminate(&host.0);
        assert_eq!(exit_code(&mut host.0).await, 0);
    }
    record("host_cold_start", "ms", &samples);
}

/// PERF-index-snapshot: with the host running, from connecting to the last
/// page of the index's snapshot of 200 sessions.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "PERF-index-snapshot runs nightly"]
async fn perf_index_snapshot_of_two_hundred_sessions() {
    let dir = TempDir::new("perf-index");
    let data_dir = seeded(&dir).await;
    let _host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    connect(&data_dir).await;
    let clock = SystemClock::new();
    let mut samples = Vec::new();
    for _ in 0..SAMPLES {
        let start = clock.now_mono();
        let mut client = connect_once(&data_dir, harness()).await.unwrap();
        assert_eq!(index_snapshot(&mut client).await, 200);
        samples.push(ms(start, clock.now_mono()));
    }
    record("index_snapshot", "ms", &samples);
}

/// PERF-host-memory (NFR-11): the host's resident memory while six sessions
/// run a turn with output each, every 100 ms, and after, until there are
/// enough samples. The adapter is a process of its own and not counted.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "PERF-host-memory runs nightly"]
async fn perf_host_memory_with_six_active_sessions() {
    let dir = TempDir::new("perf-memory");
    let data_dir = fake_agent_data(&dir);
    let host = Running(spawn_with(&data_dir, fake_agent_config(json!({}))));
    let pid = i32::try_from(host.0.id()).unwrap();
    let client = connect(&data_dir).await;
    let script = "flood_mixed 300 50; flood 2000 50; tool t1 read Read notes; \
                  tool_done t1 30000; say done";
    for _ in 0..6 {
        create(&client, script).await;
    }
    let completed = "SELECT CAST(count(*) AS TEXT) FROM turns WHERE status = 'completed'";
    let clock = SystemClock::new();
    let give_up = clock.now_mono() + PATIENCE;
    let mut samples = Vec::new();
    while samples.len() < SAMPLES || column(&data_dir, completed) != ["6"] {
        assert!(clock.now_mono() < give_up, "the six turns did not end");
        let bytes = cupper_sys::resident_size(pid).expect("the host runs");
        samples.push(bytes as f64 / f64::from(1 << 20));
        clock
            .timer_at(clock.now_mono() + Duration::from_millis(100))
            .await;
    }
    record("host_memory", "MiB", &samples);
}
