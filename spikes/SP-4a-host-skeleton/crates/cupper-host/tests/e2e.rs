//! Automated acceptance tests 1-7 of SP-4a, against a real host process
//! (test-harness build) and the scripted fake agent.
//!
//! Run: `cargo test -p cupper-host --features test-harness --test e2e`

mod support;

use std::time::{Duration, Instant};

use cupper_cli::command_id;
use cupper_host::store::Store;
use cupper_host::sys;
use cupper_protocol::rpc::ErrorCode;
use cupper_protocol::sync::StreamId;
use cupper_protocol::*;
use support::*;

/// View equality with a short diagnostic instead of a multi-megabyte Debug dump.
fn assert_same(a: &cupper_protocol::view::StreamView, b: &cupper_protocol::view::StreamView) {
    if a != b {
        let (ja, jb) = (serde_json::to_value(a).unwrap(), serde_json::to_value(b).unwrap());
        let summary = |v: &serde_json::Value| {
            let n = |k: &str| v.get(k).and_then(|x| x.as_array()).map_or(0, |x| x.len());
            format!("turns={} items={} interactions={} sessions={}", n("turns"), n("items"), n("interactions"), n("sessions"))
        };
        panic!("views differ: {} vs {}", summary(&ja), summary(&jb));
    }
}

/// 1. Two clients subscribed at the same time see identical content; a late
///    subscriber's snapshot matches too. Large tool output arrives as a blob.
#[tokio::test(flavor = "multi_thread")]
async fn t1_two_clients_see_the_same_stream() {
    let h = Harness::new(&[]);
    let mut a = h.client().await;
    let mut b = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;
    b.subscribe(&ss(&sid), None).await;
    a.subscribe("index", None).await;
    b.subscribe("index", None).await;

    let turn = a.send(&sid, "flood_mixed 300 40; tool t1 read Read big file; tool_done t1 40000; say done").await;
    assert_eq!(a.wait_turn(&sid, &turn).await, TurnStatus::Completed);
    assert_eq!(b.wait_turn(&sid, &turn).await, TurnStatus::Completed);
    let idle = |m: &std::collections::HashMap<String, cupper_cli::Mirror>| {
        m["index"].index().is_some_and(|i| i.sessions.iter().any(|s| s.status == SessionStatus::Idle))
    };
    a.pump_until(idle).await;
    b.pump_until(idle).await;

    assert_same(&a.mirror(&ss(&sid)).view, &b.mirror(&ss(&sid)).view);
    assert_same(&a.mirror("index").view, &b.mirror("index").view);
    assert!(turn_text(a.session_view(&sid), &turn).ends_with("done"));

    let mut late = h.client().await;
    late.subscribe(&ss(&sid), None).await;
    late.pump_until(|m| m[&ss(&sid)].last_seq().is_some()).await;
    assert_same(&late.mirror(&ss(&sid)).view, &a.mirror(&ss(&sid)).view);

    // The 40,000-char tool output is above the inline threshold: blob reference.
    let tool = a.session_view(&sid).items.iter().find(|i| i.kind == ItemKind::ToolCall).expect("tool item").clone();
    let Some(Content::Blob { blob, preview }) = tool.content else { panic!("expected blob content, got {:?}", tool.content) };
    assert_eq!(blob.size, 40_000);
    assert!(preview.chars().count() <= 201);
    let bytes = a.fetch_blob(&blob.hash).await;
    assert_eq!(bytes, vec![b'a'; 40_000]);
}

/// 2. Approvals are first come, first served; the loser gets `conflict`.
#[tokio::test(flavor = "multi_thread")]
async fn t2_first_answer_wins() {
    let h = Harness::new(&[]);
    let mut a = h.client().await;
    let mut b = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;
    b.subscribe(&ss(&sid), None).await;
    let turn = a.send(&sid, "tool t1 execute Run tests; permission t1; say finished").await;
    let (rid, digest) = a.wait_pending(&sid).await;
    let (rid_b, digest_b) = b.wait_pending(&sid).await;
    assert_eq!((&rid, &digest), (&rid_b, &digest_b));

    // Guards before the race: read-only principal, allow_always, stale digest.
    let ro = h.readonly_client().await;
    let e = ro.respond(&sid, &rid, &digest, InteractionOutcome::Selected { option_id: "allow".into() }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::Forbidden));
    let e = a.respond(&sid, &rid, &digest, InteractionOutcome::Selected { option_id: "always".into() }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::Forbidden));
    let e = a.respond(&sid, &rid, "d_wrong", InteractionOutcome::Selected { option_id: "allow".into() }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::Conflict));

    let allow = InteractionOutcome::Selected { option_id: "allow".into() };
    let (ra, rb) = tokio::join!(a.respond(&sid, &rid, &digest, allow.clone()), b.respond(&sid, &rid, &digest, allow.clone()));
    let lost = match (ra, rb) {
        (Ok(_), Err(e)) | (Err(e), Ok(_)) => e,
        other => panic!("expected exactly one winner, got {other:?}"),
    };
    let cupper_cli::ClientError::Rpc(err) = &lost else { panic!("{lost}") };
    let data = err.data.as_ref().expect("error data");
    assert_eq!(data.code, ErrorCode::Conflict);
    assert_eq!(data.reason.as_deref(), Some("already_resolved"));
    assert_eq!(data.extra["resolved_by"], "p_test_harness");

    assert_eq!(a.wait_turn(&sid, &turn).await, TurnStatus::Completed);
    b.wait_turn(&sid, &turn).await;
    let text = turn_text(a.session_view(&sid), &turn);
    assert!(text.contains("permission:allow"), "{text}");
    let resolved = a.session_view(&sid).interaction(&rid).expect("interaction").clone();
    assert_eq!(resolved.status, InteractionStatus::Resolved);
    assert_same(&a.mirror(&ss(&sid)).view, &b.mirror(&ss(&sid)).view);

    // Questions go through the same path.
    let turn = a.send(&sid, "question Which branch?").await;
    let (rid, digest) = a.wait_pending(&sid).await;
    a.respond(&sid, &rid, &digest, InteractionOutcome::Answer { text: "main".into() }).await.expect("answer");
    a.wait_turn(&sid, &turn).await;
    assert!(turn_text(a.session_view(&sid), &turn).contains("answer:main"));
}

/// 3. Events produced while a client is away are replayed on reconnect with no
///    loss and no duplicates (the mirror enforces `from == last + 1`). A cursor
///    older than the retained log gets a snapshot instead.
#[tokio::test(flavor = "multi_thread")]
async fn t3_reconnect_replays_without_gaps() {
    let h = Harness::new(&["--retain-events", "200"]);
    let writer = h.client().await;
    let sid = writer.create_session().await;
    let mut a = h.client().await;
    a.subscribe(&ss(&sid), None).await;
    let t1 = writer.send(&sid, "say first").await;
    a.wait_turn(&sid, &t1).await;
    let mirror = a.mirrors.remove(&ss(&sid)).unwrap();
    let cursor = mirror.last_seq().expect("cursor");
    drop(a);

    // While "a" is away: a long turn with many deltas.
    let mut w = writer;
    w.subscribe(&ss(&sid), None).await;
    let t2 = w.send(&sid, "flood 3000 7; think pondering; flood 50 3; say end").await;
    w.wait_turn(&sid, &t2).await;
    let head = w.mirror(&ss(&sid)).last_seq().unwrap();
    assert!(head > cursor + 5, "head {head} cursor {cursor}");

    let mut a = h.client().await;
    a.mirrors.insert(ss(&sid), mirror);
    let r = a.subscribe(&ss(&sid), Some(cursor)).await;
    assert_eq!(r.mode, SubscribeMode::Replay);
    a.pump_until(|m| m[&ss(&sid)].last_seq() == Some(head)).await;
    let m = a.mirror(&ss(&sid));
    assert_same(&m.view, &w.mirror(&ss(&sid)).view);
    assert!(m.ranges > 0, "replay should coalesce consecutive deltas into range events");
    assert_eq!(turn_text(a.session_view(&sid), &t2).len(), 3000 * 7 + 50 * 3 + 3);

    // Push the log past retention, then come back with an ancient cursor.
    for _ in 0..4 {
        let t = w.send(&sid, "flood_mixed 60 5").await;
        w.wait_turn(&sid, &t).await;
    }
    let head = w.mirror(&ss(&sid)).last_seq().unwrap();
    let mut old = h.client().await;
    let r = old.subscribe(&ss(&sid), Some(1)).await;
    assert_eq!(r.mode, SubscribeMode::Snapshot, "cursor 1 is older than the retained log");
    old.pump_until(|m| m[&ss(&sid)].last_seq() == Some(head)).await;
    assert_same(&old.mirror(&ss(&sid)).view, &w.mirror(&ss(&sid)).view);

    // A client that negotiated a coalescing window gets range events live.
    let mut slowpoke = h.client_with(Some(200)).await;
    slowpoke.subscribe(&ss(&sid), None).await;
    slowpoke.pump_until(|m| m[&ss(&sid)].last_seq() == Some(head)).await;
    let t = w.send(&sid, "flood 400 5; sleep 300; flood 400 5").await;
    slowpoke.wait_turn(&sid, &t).await;
    w.wait_turn(&sid, &t).await;
    assert_same(&slowpoke.mirror(&ss(&sid)).view, &w.mirror(&ss(&sid)).view);
}

/// 4. A client that stops reading does not slow the agent down: the host resets
///    its subscription instead of waiting, and it catches up from its cursor.
#[tokio::test(flavor = "multi_thread")]
async fn t4_slow_client_does_not_block_the_agent() {
    let h = Harness::new(&["--max-queue-bytes", "262144"]);
    let mut fast = h.client().await;
    let sid = fast.create_session().await;
    fast.subscribe(&ss(&sid), None).await;
    let script = "flood_mixed 6000 200; say end";

    // Baseline without a stalled client.
    let t0 = Instant::now();
    let turn = fast.send(&sid, script).await;
    fast.wait_turn(&sid, &turn).await;
    let baseline = t0.elapsed();

    let mut slow = h.client().await;
    slow.subscribe(&ss(&sid), None).await;
    slow.pump_until(|m| m[&ss(&sid)].last_seq().is_some()).await;
    slow.c.pause_reading();

    let t0 = Instant::now();
    let turn = fast.send(&sid, script).await;
    assert_eq!(fast.wait_turn(&sid, &turn).await, TurnStatus::Completed);
    let with_stalled = t0.elapsed();
    println!("turn with {} events: baseline {baseline:?}, with a stalled client {with_stalled:?}", fast.mirror(&ss(&sid)).last_seq().unwrap());
    assert!(with_stalled < baseline * 3 + Duration::from_secs(2), "stalled client slowed the agent: {with_stalled:?} vs {baseline:?}");

    // The stalled client resumes: it must see a reset, resubscribe from its
    // cursor, and end up with exactly the same state.
    slow.c.resume_reading();
    slow.wait_turn(&sid, &turn).await;
    let m = slow.mirror(&ss(&sid));
    assert!(m.resets >= 1, "expected a stream/reset for the stalled client");
    assert_same(&m.view, &fast.mirror(&ss(&sid)).view);
}

/// 5. `kill -9` the host mid-turn; on restart leftover adapter processes are
///    reaped, the turn is closed as interrupted, the pending request expired,
///    and the next message resumes the same agent session.
#[tokio::test(flavor = "multi_thread")]
async fn t5_kill9_recovery_and_resume() {
    let mut h = Harness::new(&[]);
    let mut a = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;
    let t1 = a.send(&sid, "whoami").await;
    a.wait_turn(&sid, &t1).await;
    let agent_sid = turn_text(a.session_view(&sid), &t1);
    assert!(agent_sid.contains("resumed=false"), "{agent_sid}");

    // A turn that leaks a grandchild and then blocks on an approval.
    let t2 = a.send(&sid, "spawn_child; tool t1 edit Edit file; permission t1").await;
    let (rid, digest) = a.wait_pending(&sid).await;
    let mirror = a.mirrors.remove(&ss(&sid)).unwrap();
    let cursor = mirror.last_seq().unwrap();
    drop(a);
    h.kill9();

    let adapters = Store::open(&h.data_dir().join("host.db")).unwrap().adapters().unwrap();
    assert_eq!(adapters.len(), 1, "one adapter recorded");
    let pgid = adapters[0].pid;
    // The adapter sees EOF and exits, but its grandchild lingers in the group.
    let deadline = Instant::now() + Duration::from_secs(5);
    while sys::group_members(pgid).iter().any(|(p, _)| *p == pgid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    let leftovers = sys::group_members(pgid);
    assert!(!leftovers.is_empty(), "expected the leaked grandchild to survive the host");

    h.start();
    assert!(sys::group_members(pgid).is_empty(), "leftover processes after restart: {:?}", sys::group_members(pgid));

    // Reconnect with the mirror and cursor from before the crash.
    let mut a = h.client().await;
    a.mirrors.insert(ss(&sid), mirror);
    let r = a.subscribe(&ss(&sid), Some(cursor)).await;
    assert_eq!(r.mode, SubscribeMode::Replay);
    a.subscribe("index", None).await;
    a.pump_until(|m| {
        m[&ss(&sid)].session().and_then(|s| s.turn(&t2)).is_some_and(|t| t.status == TurnStatus::Interrupted)
            && m["index"].index().and_then(|i| i.session(&sid)).is_some_and(|s| s.status == SessionStatus::Interrupted)
    })
    .await;
    assert_eq!(a.session_view(&sid).interaction(&rid).unwrap().status, InteractionStatus::Expired);
    let e = a.respond(&sid, &rid, &digest, InteractionOutcome::Selected { option_id: "allow".into() }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::Expired));

    // Continue the conversation: the host starts a new adapter and resumes.
    let t3 = a.send(&sid, "whoami").await;
    assert_eq!(a.wait_turn(&sid, &t3).await, TurnStatus::Completed);
    let text = turn_text(a.session_view(&sid), &t3);
    let original = agent_sid.split_whitespace().next().unwrap();
    assert!(text.contains(original) && text.contains("resumed=true"), "{text} vs {original}");
}

/// 6. Retrying a command with the same command_id never runs it twice,
///    including concurrent duplicates and retries after a host restart.
#[tokio::test(flavor = "multi_thread")]
async fn t6_command_retries_are_idempotent() {
    let mut h = Harness::new(&[]);
    let mut a = h.client().await;
    let b = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;

    let id = command_id();
    let first = a.send_with_id(&sid, "say once", &id).await.unwrap();
    let again = a.send_with_id(&sid, "say once", &id).await.unwrap();
    assert!(!first.replayed && again.replayed);
    assert_eq!(first.turn_id, again.turn_id);
    a.wait_turn(&sid, &first.turn_id).await;

    // Concurrent duplicates from two connections.
    let id = command_id();
    let (x, y) = tokio::join!(a.send_with_id(&sid, "say twice?", &id), b.send_with_id(&sid, "say twice?", &id));
    let (x, y) = (x.unwrap(), y.unwrap());
    assert_eq!(x.turn_id, y.turn_id);
    assert!(x.replayed ^ y.replayed);
    a.wait_turn(&sid, &x.turn_id).await;

    // Same id, different parameters.
    let e = a.send_with_id(&sid, "say something else", &id).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::IdempotencyMismatch));

    // Results survive a restart.
    h.stop();
    h.start();
    let a2 = h.client().await;
    let after_restart = a2.send_with_id(&sid, "say twice?", &id).await.unwrap();
    assert!(after_restart.replayed);
    assert_eq!(after_restart.turn_id, x.turn_id);

    let mut check = h.client().await;
    check.subscribe(&ss(&sid), None).await;
    check.pump_until(|m| m[&ss(&sid)].last_seq().is_some()).await;
    let v = check.session_view(&sid);
    assert_eq!(v.turns.len(), 2, "exactly two turns ran");
    assert_eq!(v.items.iter().filter(|i| i.kind == ItemKind::UserMessage).count(), 2);
}

/// 7 (end to end). Oversized agent output still yields frames within the limit:
/// a 3 MB message is split into deltas, a 500 kB tool output becomes a blob, a
/// 100 kB title is truncated, and snapshots of all of it are paged. In the other
/// direction the client refuses to send a frame over the limit.
#[tokio::test(flavor = "multi_thread")]
async fn t7_frames_stay_within_limit() {
    let h = Harness::new(&[]);
    let mut a = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;
    let e = a.send_with_id(&sid, &"x".repeat(100_000), &command_id()).await.unwrap_err();
    assert!(matches!(e, cupper_cli::ClientError::Encode(_)), "{e}");

    let turn = a.send(&sid, "big 3000000; long_tool t1 100000; tool_done t1 500000; say end").await;
    a.wait_turn(&sid, &turn).await;
    let tool = a.session_view(&sid).items.iter().find(|i| i.kind == ItemKind::ToolCall).unwrap().clone();
    assert!(tool.title.unwrap().chars().count() <= cupper_protocol::text::MAX_LABEL_CHARS + 1);
    assert!(matches!(tool.content, Some(Content::Blob { .. })));
    let text = turn_text(a.session_view(&sid), &turn);
    assert_eq!(text.len(), 3_000_000 + 3);

    let mut late = h.client().await;
    late.subscribe(&ss(&sid), None).await;
    late.pump_until(|m| m[&ss(&sid)].last_seq().is_some()).await;
    assert_same(&late.mirror(&ss(&sid)).view, &a.mirror(&ss(&sid)).view);
    // pump_until asserts max_frame_seen <= MAX_FRAME_BYTES on both clients.
    println!("largest frame seen: {} bytes", late.c.max_frame_seen.load(std::sync::atomic::Ordering::Relaxed));
}

/// Adapter isolation (ADR-05): only fds 0-2 are inherited even though the host
/// holds the database, the socket and client connections; the environment is
/// the allow-list (no `CUPPER_*`, nothing unlisted).
#[tokio::test(flavor = "multi_thread")]
async fn adapter_gets_only_stdio_and_allowed_env() {
    let h = Harness::new(&[]);
    let mut a = h.client().await;
    let _other = h.client().await;
    let sid = a.create_session().await;
    a.subscribe(&ss(&sid), None).await;
    let t = a.send(&sid, "probe").await;
    a.wait_turn(&sid, &t).await;
    let text = turn_text(a.session_view(&sid), &t);
    let (fds, env) = text.trim_start_matches("fds=").split_once(" env=").expect("probe output");
    assert_eq!(fds, "0,1,2", "inherited fds");
    let names: Vec<&str> = env.split(',').collect();
    assert!(names.iter().all(|n| !n.starts_with("CUPPER_")), "{env}");
    assert!(!names.contains(&"UNLISTED_TEST_VAR"), "{env}");
    assert!(names.contains(&"PATH") && names.contains(&"HOME"), "{env}");
}

/// Protocol edge cases: handshake order, unknown session, unknown method.
#[tokio::test(flavor = "multi_thread")]
async fn protocol_errors() {
    let h = Harness::new(&[]);
    let stream = tokio::net::UnixStream::connect(h.socket()).await.unwrap();
    let raw = cupper_cli::Client::from_stream(stream);
    let e = raw.request::<Subscribe>(SubscribeParams { stream: "index".into(), after_seq: None }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::NotInitialized));

    let a = h.client().await;
    let e = a.c.request::<Subscribe>(SubscribeParams { stream: StreamId::Session("s_nope".into()).to_string(), after_seq: None }).await.unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::NotFound));
    let e = a.c.request_raw("git/status", serde_json::json!({})).await.unwrap_err();
    assert!(matches!(e, cupper_cli::ClientError::Rpc(ref r) if r.code == cupper_protocol::rpc::METHOD_NOT_FOUND));

    let ro = h.readonly_client().await;
    assert_eq!(ro.c.init.as_ref().unwrap().principal.kind, PrincipalKind::Cli);
    let e = ro
        .c
        .request::<SessionCreate>(SessionCreateParams { meta: CommandMeta::new(command_id()), agent: "fake".into(), cwd: "/".into() })
        .await
        .unwrap_err();
    assert_eq!(e.app_code(), Some(&ErrorCode::Forbidden));

    // Single instance: a second host on the same data dir refuses to start.
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_cupper-host"))
        .arg("--data-dir")
        .arg(h.data_dir())
        .env("CUPPER_LOG", "error")
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(3));
}
