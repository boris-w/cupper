//! Inputs and probes shared by the turn machine's tests.

use std::sync::Arc;

use cupper_protocol::domain::{PromptBlock, PromptContent, TextContent};
use cupper_protocol::event::EventBody;
use cupper_protocol::ids::{AdapterInstanceId, TurnId};
use cupper_protocol::session::{
    SnapshotPhase, SnapshotReason, SnapshotRecorded, SnapshotStatus, StopReason,
};

use super::turn::{Action, Input, SendMode, SendRequest, TurnMachine};
use crate::clock::{IdGen, ManualClock};
use crate::driver::api::{
    AgentError, AgentErrorClass, AgentErrorForm, AgentSessionId, AttachInfo, AttachTarget,
    DriverCommand, DriverEvent, PromptTag,
};
use crate::durations::Durations;
use crate::pure::{CoreDriver, TimerRequest};

pub fn machine() -> CoreDriver<TurnMachine> {
    let clock = ManualClock::new();
    let ids = Arc::new(IdGen::seeded(Arc::new(clock.clone()), 1));
    CoreDriver::with_clock(TurnMachine::new(ids, &Durations::default()), clock)
}

pub fn text(text: &str) -> PromptContent {
    vec![PromptBlock::Text(TextContent { text: text.into() })]
}

pub fn send(message: &str, mode: SendMode) -> Input {
    Input::Send(SendRequest {
        content: text(message),
        mode,
        by: "pr_test".parse().unwrap(),
        reply_to: None,
    })
}

pub fn attached(instance: u128) -> Input {
    let capabilities = serde_json::from_value(serde_json::json!({
        "agent": "claude",
        "versions": {"adapter": "1", "cli": "1", "cli_compat": "verified"},
        "prompt": {"image": true, "file_ref": true, "max_image_bytes": 1},
        "turn": {"queue": true, "steer": {"supported": false, "side_effects": false},
                 "interrupt": true, "interrupt_stops_commands": true},
        "session": {"resume": true, "load_history": false, "fork": false},
        "config": [], "commands": [],
        "interactions": {"reject_ends_turn": false, "question_forms": true, "plan_confirm": true},
        "extraction": {"exit_code": true, "output_streaming": false, "diff": "per_file",
                       "thinking": true, "plan": "markdown_only", "run_state": "inferred"}
    }))
    .unwrap();
    Input::Attached(Ok(AttachInfo {
        session: AgentSessionId("agent-1".into()),
        instance: AdapterInstanceId::from_uuid(instance),
        capabilities,
    }))
}

pub fn agent_error() -> AgentError {
    AgentError {
        class: AgentErrorClass::Other,
        form: AgentErrorForm::Rpc { agent_code: -32603 },
        message: String::new(),
    }
}

pub fn snapshot(turn_id: &TurnId, phase: SnapshotPhase) -> Input {
    Input::Snapshot(SnapshotRecorded {
        turn_id: turn_id.clone(),
        phase,
        status: SnapshotStatus::Skipped,
        reason: Some(SnapshotReason::NotARepo),
        excluded: Vec::new(),
        stat: None,
    })
}

pub fn done(tag: u64, stop_reason: StopReason) -> Input {
    Input::Driver(DriverEvent::PromptDone {
        tag: PromptTag(tag),
        stop_reason,
        usage: None,
    })
}

pub fn started(actions: &[Action]) -> TurnId {
    actions
        .iter()
        .find_map(|action| match action {
            Action::Event(EventBody::TurnStarted(started)) => Some(started.turn_id.clone()),
            _ => None,
        })
        .expect("a turn started")
}

/// One short name per action, for comparing a step's actions in order.
pub fn kinds(actions: &[Action]) -> Vec<&str> {
    actions
        .iter()
        .map(|action| match action {
            Action::Event(body) => body.tag(),
            Action::Driver(DriverCommand::Prompt { .. }) => "prompt",
            Action::Driver(DriverCommand::Cancel { .. }) => "cancel",
            Action::Driver(DriverCommand::Close { .. }) => "close",
            Action::Driver(_) => "driver",
            Action::Attach(AttachTarget::New) => "attach",
            Action::Attach(AttachTarget::Resume(_)) => "resume",
            Action::Snapshot { .. } => "snapshot",
            Action::CancelAsks(_) => "cancel_asks",
            Action::Settle { .. } => "settle",
            Action::Complete(_) => "complete",
            Action::Timer(TimerRequest::Set { .. }) => "timer",
            Action::Timer(TimerRequest::Cancel(_)) => "untimer",
            Action::Reply(_) => "reply",
            Action::Check => "check",
        })
        .collect()
}
