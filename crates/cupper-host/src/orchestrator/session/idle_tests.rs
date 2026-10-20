//! IT-orch-actor-idle: a session actor leaves memory once idle for
//! `session_idle` (design 5.2.3), and the session loads again when a call or
//! a subscription comes for it (design 5.2.13); on IT-orch-lifecycle's
//! fixture.

use std::time::Duration;

use cupper_protocol::session::StopReason;
use serde_json::json;

use super::interaction_tests::{ask, asked, once};
use super::tests::{Fixture, data, done, on, send};
use crate::driver::api::{AgentSessionId, AttachTarget, DriverCommand, PromptTag};
use crate::durations::SESSION_IDLE;
use crate::orchestrator::testkit::text;

/// Lets the idle time pass but for `short`, once everything settled.
async fn idle_for(f: &Fixture, short: Duration) {
    f.manager.settled().await;
    f.clock.advance(SESSION_IDLE.design_value() - short);
    f.manager.settled().await;
}

#[tokio::test]
async fn it_orch_actor_idle_leaves_memory_and_the_session_loads_again() {
    let mut f = Fixture::new("idle").await;
    let id = f.started().await;
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&id, "turn.completed", 1).await;
    idle_for(&f, Duration::from_secs(1)).await;
    assert!(f.manager.in_memory(&id) && f.quiet());
    // It closes its agent session as it leaves.
    f.clock.advance(Duration::from_secs(1));
    assert!(matches!(f.command().await, DriverCommand::Close { .. }));
    f.manager.settled().await;
    assert!(!f.manager.in_memory(&id));

    let sent = f.ok("session/send", send(&id, "s-1", "again")).await;
    assert_eq!(sent["disposition"], "started");
    let resume = AttachTarget::Resume(AgentSessionId("agent-1".into()));
    assert_eq!(f.attach().await.target, resume);
    assert_eq!(f.prompt().await.1, text("again"));
}

#[tokio::test]
async fn it_orch_actor_idle_waits_for_the_turn_and_not_for_a_paused_queue() {
    let mut f = Fixture::new("busy").await;
    let id = f.started().await;
    // A running turn keeps it.
    idle_for(&f, Duration::ZERO).await;
    assert!(f.manager.in_memory(&id));
    // Interrupted, the queue pauses: it waits for the user, not the actor.
    f.ok("session/send", send(&id, "s-1", "next")).await;
    f.ok("session/interrupt", on(&id, "i-1")).await;
    assert!(matches!(f.command().await, DriverCommand::Cancel { .. }));
    f.emit(done(PromptTag(1), StopReason::Cancelled));
    f.events(&id, "turn.completed", 1).await;
    idle_for(&f, Duration::ZERO).await;
    assert!(!f.manager.in_memory(&id));
    assert!(matches!(f.command().await, DriverCommand::Close { .. }));
    f.ok("session/queue_resume", on(&id, "r-1")).await;
    f.attach().await;
    assert_eq!(f.prompt().await.1, text("next"));
}

#[tokio::test]
async fn it_orch_actor_idle_an_exit_and_calls_at_once_keep_one_actor() {
    let mut f = Fixture::new("exit-and-call").await;
    let id = f.started().await;
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&id, "turn.completed", 1).await;
    idle_for(&f, Duration::from_secs(1)).await;
    // The timer fires as the calls arrive: the calls go to one actor, the
    // one leaving or the one loading after it.
    f.clock.advance(Duration::from_secs(1));
    let get = json!({"session_id": id});
    let (one, two) = tokio::join!(f.ok("session/get", get.clone()), f.ok("session/get", get));
    assert_eq!(one, two);
    let sent = f.ok("session/send", send(&id, "s-1", "again")).await;
    assert_eq!(sent["disposition"], "started");
}

#[tokio::test]
async fn it_orch_actor_idle_a_subscription_after_a_restart_loads_the_session() {
    let mut f = Fixture::new("subscribed").await;
    let id = f.started().await;
    f.emit(ask(1, vec![once()]));
    asked(&f, &json!(id), 1).await;
    f.restart().await;

    // No command: the subscription alone brings what the load writes.
    f.subscribe(&id);
    let bodies = f.events(&id, "turn.completed", 1).await;
    assert_eq!(data(&bodies, "turn.completed")[0]["status"], "interrupted");
    assert_eq!(
        data(&bodies, "interaction.resolved")[0]["status"],
        "expired"
    );
}
