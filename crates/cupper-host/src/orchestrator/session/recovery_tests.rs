//! IT-orch-recovery: sessions loaded from the store after the host stopped
//! without its orderly exit (design 5.2.6, 5.2.13), on IT-orch-lifecycle's
//! fixture: what ran ends in the stream, the queue waits, the session goes
//! on with its agent session, queue and requests, its setter and its choice
//! about the project's configuration.

use cupper_protocol::ids::SessionId;
use cupper_protocol::session::StopReason;
use cupper_protocol::view::Window;
use serde_json::json;

use super::interaction_tests::{answer, answered, ask, asked, once, selected};
use super::tests::{Fixture, caps, create, data, done, send, tags};
use crate::driver::api::{
    AgentSessionId, AttachTarget, DriverCommand, ProjectConfigPolicy, PromptTag,
};
use crate::orchestrator::testkit::text;

#[tokio::test]
async fn it_orch_recovery_a_restart_ends_what_ran_and_the_session_goes_on() {
    let mut f = Fixture::new("restart").await;
    let id = f.started().await;
    f.emit(ask(1, vec![once()]));
    f.emit(ask(2, vec![once()]));
    let asked = asked(&f, &json!(id), 2).await;
    let dismissed = json!({"type": "dismissed"});
    f.ok("interaction/respond", answer(&asked[0], dismissed, "r-1"))
        .await;
    let mut queued = send(&id, "s-1", "rather this");
    queued["reply_to"] = asked[0]["request_id"].clone();
    f.ok("session/send", queued).await;
    f.restart().await;

    // The first call loads the session: the pending request expires, then
    // the turn completes `interrupted`, and the queue waits for the user.
    let get = json!({"session_id": id, "request_id": asked[1]["request_id"]});
    let expired = f.ok("interaction/get", get).await;
    assert_eq!(
        [&expired["status"], &expired["reason"]],
        ["expired", "host_restarted"]
    );
    let bodies = f.events(&id, "turn.completed", 1).await;
    let all = tags(&bodies);
    assert!(all.ends_with("interaction.resolved turn.completed queue.updated"));
    assert_eq!(data(&bodies, "turn.completed")[0]["status"], "interrupted");
    let queue = data(&bodies, "queue.updated").pop().unwrap();
    assert_eq!(queue["paused"]["reason"], "turn_interrupted");
    let late = answer(&asked[1], selected("once"), "r-2");
    assert_eq!(
        f.err("interaction/respond", late).await,
        "expired host_restarted"
    );
    // `session/get` and the index see the same: the index's rows and notes.
    let got = f.ok("session/get", json!({"session_id": id})).await;
    assert_eq!(got["summary"]["status"], "unread");
    assert_eq!(f.index.0.lock().unwrap().as_slice(), [0]);
    let ended = all.split(' ').position(|t| t == "turn.completed").unwrap() + 1;
    let state = "SELECT state || ' ' || last_turn_end_seq FROM sessions";
    assert_eq!(f.rows(state), [format!("interrupted {ended}")]);

    // A message resumes the agent's session; the queued one follows, with
    // its command and the request it answers.
    let sent = f.ok("session/send", send(&id, "s-2", "go on")).await;
    assert_eq!(sent["disposition"], "started");
    let resume = AttachTarget::Resume(AgentSessionId("agent-1".into()));
    assert_eq!(f.attach().await.target, resume);
    assert_eq!(f.prompt().await, (PromptTag(1), text("go on")));
    let reply_to = asked[0]["request_id"].as_str().unwrap();
    let entries = "SELECT command_id || ' ' || reply_to FROM queue_entries";
    assert_eq!(f.rows(entries), [format!("s-1 {reply_to}")]);
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    assert_eq!(f.prompt().await.1, text("rather this"));
    let bodies = f.events(&id, "turn.started", 3).await;
    let numbers: Vec<_> = (data(&bodies, "turn.started").iter())
        .map(|t| t["turn_no"].clone())
        .collect();
    assert_eq!(numbers, [1, 2, 3]);
    let message = data(&bodies, "item.started").pop().unwrap();
    assert_eq!(message["reply_to"], reply_to);
}

#[tokio::test]
async fn it_orch_recovery_interaction_get_reads_a_request_the_view_let_go() {
    let mut f = Fixture::new("evicted").await;
    let id = f.started().await;
    f.emit(ask(1, vec![once()]));
    let asked = asked(&f, &json!(id), 1).await;
    f.ok(
        "interaction/respond",
        answer(&asked[0], selected("once"), "r-1"),
    )
    .await;
    answered(&mut f).await;
    // The window's worth of later turns pushes the first out of the view.
    let turns = u64::try_from(Window::V1.turns).unwrap() + 1;
    for n in 1..turns {
        f.emit(done(PromptTag(n), StopReason::EndTurn));
        f.events(&id, "turn.completed", n as usize).await;
        f.ok("session/send", send(&id, &format!("s-{n}"), "more"))
            .await;
        f.prompt().await;
    }
    f.emit(done(PromptTag(turns), StopReason::EndTurn));
    f.events(&id, "turn.completed", turns as usize).await;
    f.restart().await;

    let get = json!({"session_id": id, "request_id": asked[0]["request_id"]});
    let got = f.ok("interaction/get", get).await;
    assert_eq!(
        [&got["status"], &got["outcome"]["option_id"]],
        ["resolved", "once"]
    );
    assert_eq!(got["requested"]["request_id"], asked[0]["request_id"]);
}

#[tokio::test]
async fn it_orch_recovery_the_setter_comes_back_by_its_id() {
    // An ID the host does not know counts as the lowest ceiling: `auto`
    // (elevated) then needs a confirmation. The desktop sets `elevated`.
    for (setter, unconfirmed) in [("pr_gone", true), ("pr_desktop", false)] {
        let mut f = Fixture::new("setter").await;
        let mut params = create("c-1");
        params["config"] = json!({"permission_mode": "auto"});
        let created = f.ok("session/create", params).await;
        let id: SessionId = serde_json::from_value(created["session_id"].clone()).unwrap();
        let mut auto = caps();
        auto.config[0].current = "auto".into();
        f.attach_with(auto).await;
        let (tag, _) = f.prompt().await;
        f.emit(done(tag, StopReason::EndTurn));
        f.events(&id, "turn.completed", 1).await;
        f.exec("UPDATE sessions SET mode_set_by = ?1", setter.into());
        f.restart().await;

        // Calls at once load one actor: a second would find the stream's
        // writer taken.
        let session = json!({"session_id": id});
        let (one, two) = tokio::join!(
            f.ok("session/get", session.clone()),
            f.ok("session/get", session)
        );
        assert_eq!(one, two);
        let sent = f.ok("session/send", send(&id, "s-1", "next")).await;
        let disposition = ["started", "queued"][usize::from(unconfirmed)];
        assert_eq!(sent["disposition"], disposition, "{setter}");
        let bodies = f.events(&id, "session.configured", 1).await;
        let configured = data(&bodies, "session.configured").pop().unwrap();
        let reason = &configured["confirmation_required"]["reason"];
        assert_eq!(*reason == "tier_above_setter", unconfirmed, "{setter}");
    }
}

#[tokio::test]
async fn it_orch_recovery_going_on_without_the_project_config_outlives_a_restart() {
    let mut f = Fixture::new("ignored").await;
    let id = f.started().await;
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    f.events(&id, "turn.completed", 1).await;
    f.manager.ignore_project_config(&id).await.unwrap();
    assert!(matches!(f.command().await, DriverCommand::Close { .. }));
    f.restart().await;

    // The project is trusted, and its configuration still stays out.
    f.ok("session/send", send(&id, "s-1", "next")).await;
    let attach = f.attach().await;
    assert_eq!(attach.project_config, ProjectConfigPolicy::Ignore);
}
