//! IT-orch-long-output: a tool's output over the inline limit (design
//! 5.3.5) turns `truncated`, shows its tail at most once a second, and ends
//! as a blob; on IT-orch-lifecycle's fixture.

use std::time::Duration;

use cupper_protocol::event::EventBody;
use cupper_protocol::session::StopReason;
use serde_json::{Value, json};

use super::tests::{Fixture, data, done};
use crate::driver::api::{DriverEvent, NormUpdate, PromptTag, ToolOutput, ToolStatus};

/// More output of tool `t1`, which the first starts, or its end.
fn update(output: Option<String>, status: Option<ToolStatus>) -> DriverEvent {
    DriverEvent::Update(NormUpdate::ToolUpdate {
        tool_call_id: "t1".into(),
        title: None,
        command: None,
        paths: None,
        status,
        output: output.map(ToolOutput::Append),
        exit_code: None,
        diffs: None,
    })
}

/// The tool item's events: each type with its data.
fn tool_events(bodies: &[EventBody]) -> Vec<(String, Value)> {
    let tool = data(bodies, "item.started")[1]["item_id"].clone();
    let ours = bodies
        .iter()
        .map(|b| (b.tag().to_owned(), json!(b)["data"].clone()));
    ours.filter(|(_, data)| data["item_id"] == tool).collect()
}

#[tokio::test]
async fn it_orch_long_output_turns_truncated_ticks_and_ends_as_a_blob() {
    let mut f = Fixture::new("long-output").await;
    let id = f.started().await;
    // The first piece goes out at once, the second waits in the window, the
    // third passes the limit of 16,000.
    let pieces = ["a".repeat(100), "b".repeat(10_000), "c".repeat(10_000)];
    for piece in &pieces {
        f.emit(update(Some(piece.clone()), None));
    }
    let bodies = f.events(&id, "item.updated", 2).await;
    let events = tool_events(&bodies);
    let tags: Vec<&str> = events.iter().map(|(tag, _)| tag.as_str()).collect();
    let expected = "item.started item.updated item.delta item.delta item.updated";
    assert_eq!(tags.join(" "), expected, "the buffered delta first");
    let truncated = &events[4].1;
    assert_eq!(truncated["output_state"], "truncated");
    assert_eq!(truncated["output_bytes"], 20_100);
    assert_eq!(truncated["output_tail"], "c".repeat(4_000));

    // More within the second waits for its tick; nothing more, no tick.
    f.emit(update(Some("d".repeat(5_000)), None));
    f.manager.settled().await;
    let events = tool_events(&f.events(&id, "item.updated", 2).await);
    assert_eq!(events.len(), 5);
    for _ in 0..2 {
        f.clock.advance(Duration::from_secs(1));
        f.manager.settled().await;
    }
    let bodies = f.events(&id, "item.updated", 3).await;
    let events = tool_events(&bodies);
    assert_eq!(events.len(), 6, "one tick wrote, the next had nothing");
    let tail = &events[5].1["output_tail"];
    assert_eq!(*tail, "d".repeat(4_000));
    assert_eq!(events[5].1["output_bytes"], 25_100);

    // At its end the output is a blob with the tail as preview.
    f.emit(update(None, Some(ToolStatus::Completed)));
    let bodies = f.events(&id, "item.completed", 2).await;
    let completed = &tool_events(&bodies).pop().unwrap().1;
    assert_eq!(completed["output_state"], "complete");
    let output = &completed["output"];
    assert_eq!(output["preview"], *tail);
    let whole: String = pieces.concat() + &"d".repeat(5_000);
    assert_eq!(output["blob"]["size"], whole.len());
    assert_eq!(f.blob(&output["blob"]["hash"]).await, whole.as_bytes());
    let kept = "SELECT count(*) || '' FROM blob_refs";
    assert_eq!(f.rows(kept), ["1"]);
}

#[tokio::test]
async fn it_orch_long_output_a_turn_end_leaves_it_abandoned_with_its_tail() {
    let mut f = Fixture::new("long-abandoned").await;
    let id = f.started().await;
    f.emit(update(Some("e".repeat(20_000)), None));
    f.emit(done(PromptTag(1), StopReason::EndTurn));
    let bodies = f.events(&id, "turn.completed", 1).await;
    let ended = &tool_events(&bodies).pop().unwrap().1;
    assert_eq!(ended["status"], "abandoned");
    assert_eq!(ended["output_tail"], "e".repeat(4_000));
    assert_eq!(ended["output_bytes"], 20_000);
    assert!(ended["output"].is_null(), "no blob");
}
