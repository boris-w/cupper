//! SP-4a measurements (ignored by default; run in release mode):
//!
//! `cargo test --release -p cupper-host --features test-harness --test measure -- --ignored --nocapture --test-threads 1`

mod support;

use std::time::{Duration, Instant};

use cupper_cli::Incoming;
use cupper_protocol::*;
use support::*;

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Connect, initialize, subscribe to the index and wait for the last snapshot
/// page. Returns the number of sessions in the snapshot.
async fn index_snapshot(h: &Harness) -> usize {
    let mut c = h.readonly_client().await.c;
    c.request::<Subscribe>(SubscribeParams { stream: "index".into(), after_seq: None }).await.unwrap();
    let mut sessions = 0;
    loop {
        match c.next_timeout(TIMEOUT).await.unwrap() {
            Incoming::Snapshot(p) => {
                sessions += p.ops.len();
                if p.last {
                    return sessions;
                }
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}

/// Cold start: from spawning the host process to the client holding the index
/// snapshot, on a data dir with 20 sessions of history.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn measure_cold_start() {
    let mut h = Harness::new(&[]);
    let mut a = h.client().await;
    for _ in 0..20 {
        let sid = a.create_session().await;
        a.subscribe(&ss(&sid), None).await;
        for _ in 0..3 {
            let t = a.send(&sid, "flood_mixed 100 80; tool t1 read Read; tool_done t1 2000; say done").await;
            a.wait_turn(&sid, &t).await;
        }
    }
    drop(a);
    h.stop();
    let db = std::fs::metadata(h.data_dir().join("host.db")).map(|m| m.len()).unwrap_or(0);

    let (mut cold, mut accept, mut warm) = (Vec::new(), Vec::new(), Vec::new());
    for _ in 0..30 {
        let t0 = Instant::now();
        h.spawn();
        h.wait_ready(Duration::from_millis(1));
        accept.push(t0.elapsed());
        assert_eq!(index_snapshot(&h).await, 20);
        cold.push(t0.elapsed());
        let t1 = Instant::now();
        index_snapshot(&h).await;
        warm.push(t1.elapsed());
        h.stop();
    }
    println!("data dir: 20 sessions, 60 turns, host.db {} KiB", db / 1024);
    for (name, v) in [
        ("spawn -> socket accepts", &mut accept),
        ("spawn -> index snapshot (cold)", &mut cold),
        ("connect -> index snapshot (host running)", &mut warm),
    ] {
        v.sort();
        println!(
            "{name}: n={} p50={:.1} ms p95={:.1} ms max={:.1} ms",
            v.len(),
            ms(percentile(v, 0.5)),
            ms(percentile(v, 0.95)),
            ms(*v.last().unwrap())
        );
    }
}

/// Resident memory of the host process: idle, then with 6 fake sessions.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn measure_memory() {
    let h = Harness::new(&[]);
    let pid = h.host_pid();
    let mut a = h.client().await;
    a.subscribe("index", None).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    let idle = rss_kib(pid);

    let mut sessions = Vec::new();
    for _ in 0..6 {
        let sid = a.create_session().await;
        a.subscribe(&ss(&sid), None).await;
        sessions.push(sid);
    }
    let created = rss_kib(pid);
    for sid in &sessions {
        let t = a.send(sid, "flood_mixed 2000 50; tool t1 read Read; tool_done t1 30000; say done").await;
        a.wait_turn(sid, &t).await;
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    let after_turns = rss_kib(pid);
    let adapters = cupper_host::store::Store::open(&h.data_dir().join("host.db")).unwrap().adapters().unwrap();
    let adapter_rss: u64 = adapters.iter().map(|a| rss_kib(a.pid)).sum();
    let mib = |k: u64| k as f64 / 1024.0;
    println!("host RSS idle (1 client): {:.1} MiB", mib(idle));
    println!("host RSS, 6 sessions created (6 adapters running): {:.1} MiB", mib(created));
    println!("host RSS, 6 sessions after a ~110 kB / 6,000-item turn each, all subscribed: {:.1} MiB", mib(after_turns));
    println!("{} fake adapters together (not part of the host): {:.1} MiB", adapters.len(), mib(adapter_rss));
}
