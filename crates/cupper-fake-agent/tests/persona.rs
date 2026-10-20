//! UNIT-replay: the personas look like the SP-1 recordings they follow
//! (claude-agent-acp 0.91.0 with Claude 2.1.296, codex-acp 2.2.2 with Codex
//! 0.160.1), read the way the jq commands of the T051 notes read them:
//! capabilities, config options, default mode, commands and the option IDs
//! of permission requests.

use std::path::Path;

use cupper_fake_agent::config::Persona::{self, Claude, Codex};
use cupper_fake_agent::persona;
use cupper_protocol::recording::{Body, Recording};
use serde_json::{Value, json};

/// The adapter's messages in recording `scenario` of the persona's versions.
fn received(p: Persona, scenario: &str) -> Vec<Value> {
    let versions = match p {
        Claude => "claude-agent-acp/0.91.0/2.1.296",
        Codex => "codex-acp/2.2.2/0.160.1",
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/recordings");
    let text = std::fs::read_to_string(dir.join(versions).join(format!("{scenario}.jsonl")));
    let frames = Recording::parse(&text.unwrap()).unwrap().frames.into_iter();
    let recv = |body| match body {
        Body::Recv(message) => Some(message),
        _ => None,
    };
    frames.filter_map(|frame| recv(frame.body)).collect()
}

fn first(messages: &[Value], what: impl Fn(&Value) -> bool) -> &Value {
    messages.iter().find(|m| what(m)).expect("in the recording")
}

/// Id, category and current value of each option; the model's value is the
/// recording account's, so it is left out.
fn config(options: &Value) -> Vec<Value> {
    let options = options.as_array().unwrap().iter();
    let current = |o: &Value| (o["category"] != "model").then(|| o["currentValue"].clone());
    options
        .map(|o| json!([o["id"], o["category"], current(o)]))
        .collect()
}

/// The option ID and kind of each option.
fn options(options: &Value) -> Vec<Value> {
    let options = options.as_array().unwrap().iter();
    options.map(|o| json!([o["optionId"], o["kind"]])).collect()
}

#[test]
fn unit_replay_personas_match_the_sp1_recordings() {
    for p in [Claude, Codex] {
        let s01 = received(p, "s01-init-session");
        // Capabilities, and steering in the top-level `_meta`.
        let init = &first(&s01, |m| m["id"] == 1)["result"];
        let ours = persona::initialize(p);
        assert_eq!(
            ours["agentCapabilities"], init["agentCapabilities"],
            "{p:?}"
        );
        assert_eq!(ours["_meta"], init["_meta"], "{p:?}");
        // Config options, the modes among them, and the default mode.
        let new = &first(&s01, |m| m["result"]["sessionId"].is_string())["result"];
        let ours = Value::from(persona::config_options(p));
        assert_eq!(config(&ours), config(&new["configOptions"]), "{p:?}");
        let modes = |options: &Value| options[0]["options"].clone();
        assert_eq!(modes(&ours), modes(&new["configOptions"]), "{p:?}");
        let result = persona::session_result(p, None, &persona::config_options(p));
        assert_eq!(
            result["modes"]["currentModeId"], new["modes"]["currentModeId"],
            "{p:?}"
        );
        // Commands: each of ours as recorded and in order; the recording
        // machine had more.
        let commands = |update: &Value| update["availableCommands"].as_array().unwrap().clone();
        let update = |m: &Value| m["params"]["update"]["availableCommands"].is_array();
        let recorded = commands(&first(&s01, update)["params"]["update"]);
        let mut recorded = recorded.iter();
        for command in commands(&persona::commands(p)) {
            assert!(recorded.any(|r| *r == command), "{p:?}: {command}");
        }
        // Permission options by tool kind; `switch_mode` confirms a plan.
        let mut kinds = Vec::new();
        for scenario in ["s04-approval", "s04b-reject-command", "s05-questions"] {
            let messages = received(p, scenario);
            let asks = messages
                .iter()
                .filter(|m| m["method"] == "session/request_permission");
            for ask in asks {
                let kind = ask["params"]["toolCall"]["kind"].as_str().unwrap();
                let ours = match kind {
                    "execute" => &persona::data(p)["permissionOptions"]["execute"],
                    "switch_mode" => &persona::data(p)["planOptions"],
                    _ => &persona::data(p)["permissionOptions"]["edit"],
                };
                let theirs = options(&ask["params"]["options"]);
                assert_eq!(options(ours), theirs, "{p:?} {scenario}");
                kinds.push(kind.to_owned());
            }
        }
        kinds.sort();
        kinds.dedup();
        assert_eq!(
            kinds,
            ["edit", "execute", "switch_mode"],
            "{p:?}: every kind compared"
        );
    }
}
