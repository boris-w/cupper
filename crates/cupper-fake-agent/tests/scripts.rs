//! UNIT-fake-scripts: every script command, the persona shapes on the wire,
//! the raw line channel and the global invariant, driven over the fake
//! agent's stdio pipes by a minimal ACP client.

mod common;

use std::time::Duration;

use common::{Fake, ids, is, request, says, selected, status, stop, update};
use cupper_fake_agent::agent::EXIT_SECOND_PROMPT;
use serde_json::{Value, json};

const CLAUDE: &[&str] = &[];
const CODEX: &[&str] = &["--persona", "codex"];

// Persona, start configuration and session lifecycle

#[test]
fn unit_fake_scripts_claude_persona_on_the_wire() {
    let mut fake = Fake::spawn(CLAUDE);
    let init = fake.initialize()["result"].clone();
    assert_eq!(init["_meta"]["steering"]["supported"], true);
    let agent = &init["agentInfo"]["name"];
    assert_eq!(agent, "@agentclientprotocol/claude-agent-acp");
    let auth = fake.wait_for("auth", |m| m["method"] == "_auth/status_update");
    assert_eq!(auth["params"]["authStatus"]["kind"], "account");
    let new = fake.open(Value::Null)["result"].clone();
    assert_eq!(new["modes"]["currentModeId"], "default");
    assert_eq!(
        ids(&new["configOptions"], "id"),
        ["mode", "model", "effort"]
    );
    // The slash commands follow the response (s01).
    let commands = fake.wait_for("commands", is("available_commands_update"));
    let names = ids(&update(&commands)["availableCommands"], "name");
    assert!(names.contains(&"compact".to_owned()));
    let response = fake.index(|m| m["result"]["sessionId"].is_string());
    assert!(response < fake.index(is("available_commands_update")));
}

#[test]
fn unit_fake_scripts_codex_persona_on_the_wire() {
    let mut fake = Fake::spawn(CODEX);
    let init = fake.initialize()["result"].clone();
    assert_eq!(init["agentInfo"]["name"], "@agentclientprotocol/codex-acp");
    assert!(init["agentCapabilities"]["_meta"]["claudeCode"].is_null());
    let new = fake.open(Value::Null)["result"].clone();
    assert_eq!(new["modes"]["currentModeId"], "agent");
    assert_eq!(new["models"]["currentModelId"], "gpt-6-astra[high]");
    let config = ids(&new["configOptions"], "id");
    assert_eq!(
        config,
        ["mode", "collaboration_mode", "model", "reasoning_effort"]
    );
    // A Codex turn is bracketed by its run state.
    assert_eq!(stop(&fake.prompt("say hi")), "end_turn");
    let states: Vec<&Value> = fake
        .seen
        .iter()
        .map(status)
        .filter(|s| !s.is_null())
        .collect();
    assert_eq!(
        states,
        [
            &json!({"type": "active", "activeFlags": []}),
            &json!({"type": "idle"})
        ]
    );
}

#[test]
fn unit_fake_scripts_logged_out_claude_fails_the_prompt_codex_the_session() {
    let mut claude = Fake::start(&["--auth", "logged-out"]);
    claude.index(|m| m["params"]["authStatus"]["kind"] == "none");
    assert_eq!(claude.prompt("say hi")["error"]["code"], -32000);
    let mut codex = Fake::spawn(&["--persona", "codex", "--auth", "logged-out"]);
    codex.initialize();
    assert_eq!(codex.open(Value::Null)["error"]["code"], -32000);
}

#[test]
fn unit_fake_scripts_faults_per_segment() {
    assert_eq!(Fake::spawn(&["--fault", "exit_on_start"]).exit().0, Some(1));
    let mut hang = Fake::spawn(&["--fault", "initialize_hang"]);
    let id = hang.send(
        "initialize",
        json!({"protocolVersion": 1, "clientCapabilities": {}}),
    );
    assert!(hang.quiet(300, |m| m["id"] == id));
    let mut fake = Fake::start(&["--fault", "set_config_error", "--fault", "resume_error@0"]);
    assert_eq!(fake.set_config("mode", "plan")["error"]["code"], -32603);
    assert_eq!(fake.resume()["error"]["code"], -32603);
    // Not in segment 1.
    let mut fake = Fake::start(&["--segment", "1", "--fault", "resume_error@0"]);
    assert!(fake.resume()["result"].is_object());
}

#[test]
fn unit_fake_scripts_set_config_option_keeps_the_value() {
    let mut fake = Fake::start(CODEX);
    let config = fake.set_config("mode", "read-only")["result"]["configOptions"].clone();
    assert_eq!(config[0]["currentValue"], "read-only");
    assert_eq!(fake.set_config("mode", "nope")["error"]["code"], -32602);
}

#[test]
fn unit_fake_scripts_whoami_resume_and_segment() {
    let mut fake = Fake::start(&["--segment", "3"]);
    assert_eq!(fake.session, "fake-3-1");
    fake.prompt("whoami");
    assert_eq!(fake.resume()["result"]["sessionId"], "fake-3-1");
    fake.prompt("whoami");
    let expected =
        ["resumed=false", "resumed=true"].map(|r| format!("session=fake-3-1 {r} segment=3"));
    assert_eq!(fake.texts(), expected);
}

#[test]
fn unit_fake_scripts_not_a_script_gets_a_reply() {
    let mut fake = Fake::start(CLAUDE);
    assert_eq!(stop(&fake.prompt("hello there; say x")), "end_turn");
    assert_eq!(fake.texts(), ["fake reply to: hello there; say x"]);
}

// The SP-4a commands

#[test]
fn unit_fake_scripts_text_commands() {
    let mut fake = Fake::start(CLAUDE);
    fake.prompt("say hello world; think hmm; big 5; flood 2 3; flood_mixed 2 1");
    assert_eq!(fake.texts(), ["hello world", "aaaaa", "aaa", "bbb", "a"]);
    let thoughts = fake.updates("agent_thought_chunk");
    let thoughts: Vec<&Value> = thoughts.iter().map(|u| &u["content"]["text"]).collect();
    assert_eq!(thoughts, ["hmm", "b"]);
}

#[test]
fn unit_fake_scripts_tool_commands() {
    let mut fake = Fake::start(CLAUDE);
    fake.prompt("tool t1 execute run it; long_tool t2 30; tool_done t1 4");
    let calls = fake.updates("tool_call");
    assert_eq!(
        [&calls[0]["kind"], &calls[0]["title"]],
        ["execute", "run it"]
    );
    assert_eq!(calls[1]["title"].as_str().unwrap().len(), 30);
    let done = &fake.updates("tool_call_update")[0];
    assert_eq!(
        [&done["status"], &done["content"][0]["content"]["text"]],
        ["completed", "aaaa"]
    );
}

#[test]
fn unit_fake_scripts_permission_claude_reject_goes_on() {
    let mut fake = Fake::start(CLAUDE);
    fake.answers
        .extend([selected("allow-with-updates"), selected("reject")]);
    let response = fake.prompt("permission t1; permission t2 edit; say after");
    let asked = fake
        .seen
        .iter()
        .filter(|m| request("session/request_permission")(m));
    let asked: Vec<&Value> = asked.map(|m| &m["params"]).collect();
    assert_eq!(
        ids(&asked[0]["options"], "optionId"),
        ["allow-once", "allow-with-updates", "reject"]
    );
    assert_eq!(asked[1]["toolCall"]["kind"], "edit");
    assert_eq!(
        fake.texts(),
        [
            "permission:allow-with-updates",
            "permission:reject",
            "after"
        ]
    );
    assert_eq!(fake.updates("tool_call_update")[0]["status"], "failed");
    assert_eq!(stop(&response), "end_turn");
}

#[test]
fn unit_fake_scripts_permission_codex_cancel_ends_the_turn() {
    let mut fake = Fake::start(CODEX);
    fake.answers.push_back(selected("cancel"));
    let response = fake.prompt("permission t1; say never");
    let asked = fake.index(request("session/request_permission"));
    let waiting = fake.index(|m| status(m)["activeFlags"] == json!(["waitingOnApproval"]));
    assert!(waiting < asked);
    let options = ids(&fake.seen[asked]["params"]["options"], "optionId");
    assert_eq!(
        options,
        ["allow_once", "accept_execpolicy_amendment", "cancel"]
    );
    assert_eq!(fake.texts(), ["permission:cancel"]);
    assert_eq!(stop(&response), "cancelled");
}

#[test]
fn unit_fake_scripts_permission_with_given_options() {
    let mut fake = Fake::start(CLAUDE);
    fake.answers.push_back(selected("maybe"));
    let options = r#"[{"optionId": "maybe", "name": "Maybe", "kind": "allow_maybe"}]"#;
    fake.prompt(&format!("permission_options {options}; say after"));
    let asked = fake.find(request("session/request_permission"));
    assert_eq!(ids(&asked["params"]["options"], "optionId"), ["maybe"]);
    assert_eq!(asked["params"]["options"][0]["kind"], "allow_maybe");
    assert_eq!(fake.texts(), ["permission:maybe", "after"]);
}

#[test]
fn unit_fake_scripts_cancel_withdraws_only_claudes_permission() {
    for (args, withdraws) in [(CLAUDE, true), (CODEX, false)] {
        let mut fake = Fake::start(args);
        let prompt = fake.start_prompt("permission t1");
        let asked = fake.wait_for("permission", request("session/request_permission"));
        fake.cancel();
        assert_eq!(stop(&fake.response(prompt)), "cancelled");
        let withdrawal = |m: &Value| m["method"] == "$/cancel_request";
        if withdraws {
            assert_eq!(fake.find(withdrawal)["params"]["requestId"], asked["id"]);
        } else {
            assert!(fake.quiet(200, withdrawal) && !fake.seen.iter().any(withdrawal));
        }
    }
}

#[test]
fn unit_fake_scripts_withdraw_after_a_while() {
    let mut fake = Fake::start(CLAUDE);
    let prompt = fake.start_prompt("withdraw_after 50; permission p1; say after");
    let asked = fake.wait_for("permission", request("session/request_permission"));
    let withdrawal = fake.wait_for("withdrawal", |m| m["method"] == "$/cancel_request");
    assert_eq!(withdrawal["params"]["requestId"], asked["id"]);
    assert_eq!(stop(&fake.response(prompt)), "end_turn");
    assert_eq!(fake.texts(), ["permission:cancelled", "after"]);
}

#[test]
fn unit_fake_scripts_question() {
    let mut fake = Fake::start(CLAUDE);
    fake.answers
        .push_back(json!({"action": "accept", "content": {"answer": "blue"}}));
    fake.answers.push_back(json!({"action": "decline"}));
    fake.prompt("question Pick a color");
    fake.prompt("question Again?");
    let asked = fake.find(request("elicitation/create"));
    assert_eq!(asked["params"]["message"], "Pick a color");
    assert_eq!(fake.texts(), ["answer:blue", "answer:decline"]);
}

#[test]
fn unit_fake_scripts_sleep_and_wait_cancel() {
    let mut fake = Fake::start(CLAUDE);
    assert_eq!(stop(&fake.prompt("sleep 20; say slept")), "end_turn");
    let prompt = fake.start_prompt("say before; wait_cancel; say after");
    fake.wait_for("before", says("before"));
    fake.cancel();
    assert_eq!(stop(&fake.response(prompt)), "cancelled");
    assert_eq!(fake.texts(), ["slept", "before"]);
}

#[test]
fn unit_fake_scripts_probe() {
    let mut fake = Fake::start(CLAUDE);
    fake.prompt("probe");
    let text = &fake.texts()[0];
    assert!(
        text.starts_with("fds=0,1,2") && text.contains(" env="),
        "{text}"
    );
}

#[test]
fn unit_fake_scripts_spawn_child_and_exit() {
    let mut fake = Fake::start(CLAUDE);
    assert_eq!(stop(&fake.prompt("spawn_child")), "end_turn");
    let pgrep = std::process::Command::new("pgrep")
        .args(["-P", &fake.pid().to_string()])
        .output();
    let children = String::from_utf8(pgrep.unwrap().stdout).unwrap();
    assert!(!children.trim().is_empty());
    for child in children.split_whitespace() {
        std::process::Command::new("kill")
            .arg(child)
            .status()
            .unwrap();
    }
    fake.start_prompt("exit 7");
    assert_eq!(fake.exit().0, Some(7));
}

// The commands added in design 5.9.2

#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "measures the fake agent's real-time rate"
)]
fn unit_fake_scripts_flood_rate() {
    let mut fake = Fake::start(CLAUDE);
    let started = std::time::Instant::now();
    fake.prompt("flood_rate 20 1 3");
    // 20 ticks 50 ms apart, the first one at once.
    assert!(started.elapsed() >= Duration::from_millis(900));
    let texts = fake.texts();
    assert_eq!((texts.len(), texts[1].as_str()), (20, "bbb"));
}

#[test]
fn unit_fake_scripts_terminal_per_persona() {
    let mut claude = Fake::start(CLAUDE);
    claude.prompt("terminal t1 3 2");
    assert_eq!(claude.updates("tool_call")[0]["status"], "pending");
    let updates = claude.updates("tool_call_update");
    let output = &updates[0]["_meta"]["terminal_output"]["data"];
    assert_eq!(output, "Exit code 3\nline 1\nline 2");
    let exit = json!({"terminal_id": "t1", "exit_code": 3, "signal": null});
    assert_eq!(
        [&updates[1]["status"], &updates[1]["_meta"]["terminal_exit"]],
        [&json!("failed"), &exit]
    );
    let mut codex = Fake::start(CODEX);
    codex.prompt("terminal t1 0 2");
    assert_eq!(
        codex.updates("tool_call")[0]["rawInput"]["cwd"],
        json!(codex.cwd)
    );
    let updates = codex.updates("tool_call_update");
    let data: Vec<&Value> = updates
        .iter()
        .map(|u| &u["_meta"]["terminal_output"]["data"])
        .collect();
    assert_eq!(data, [&json!("line 1\n"), &json!("line 2\n"), &Value::Null]);
    assert_eq!(updates[2]["status"], "completed");
}

#[test]
fn unit_fake_scripts_diff_per_persona() {
    for args in [CLAUDE, CODEX] {
        let mut fake = Fake::start(args);
        fake.prompt("diff d1 src/a.rs");
        let call = &fake.updates("tool_call")[0];
        let diff = &call["content"][0];
        assert_eq!([&call["kind"], &diff["type"]], ["edit", "diff"]);
        assert_eq!(diff["path"], json!(fake.cwd.join("src/a.rs")));
        // Codex tags each hunk; Claude gives a location.
        assert_eq!(diff["_meta"]["kind"] == "update", args == CODEX);
        assert_eq!(call["locations"].is_array(), args == CLAUDE);
        assert_eq!(fake.updates("tool_call_update")[0]["status"], "completed");
    }
}

#[test]
fn unit_fake_scripts_write_file_stays_in_the_workspace() {
    let mut fake = Fake::start(CLAUDE);
    assert_eq!(
        stop(&fake.prompt(r"write_file notes/a.txt hello\nworld")),
        "end_turn"
    );
    let written = std::fs::read_to_string(fake.cwd.join("notes/a.txt")).unwrap();
    assert_eq!(written, "hello\nworld");
    let outside = fake.cwd.with_extension("outside");
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, fake.cwd.join("out")).unwrap();
    for path in ["../x.txt", "/tmp/cupper-fake-x.txt", "out/x.txt"] {
        let response = fake.prompt(&format!("write_file {path} no"));
        assert_eq!(response["error"]["code"], -32602, "{path}");
    }
    assert!(!outside.join("x.txt").exists());
    std::fs::remove_dir_all(&outside).unwrap();
}

#[test]
fn unit_fake_scripts_plan_update_and_confirm_claude() {
    let mut fake = Fake::start(CLAUDE);
    fake.answers.push_back(selected("exit-plan-bypass"));
    fake.prompt(r"plan_update # Plan\n\nstep one; plan_confirm");
    let plans = fake.updates("plan_update");
    let plan = json!({"type": "markdown", "planId": "plan-mode", "content": "# Plan\n\nstep one"});
    assert_eq!((plans.len(), &plans[0]["plan"]), (2, &plan));
    let asked = fake.find(request("session/request_permission"));
    assert_eq!(
        asked["params"]["toolCall"]["rawInput"]["plan"],
        "# Plan\n\nstep one"
    );
    let mode = &fake.updates("current_mode_update")[0]["currentModeId"];
    assert_eq!(mode, "bypassPermissions");
    let config = fake.updates("config_option_update")[0]["configOptions"].clone();
    assert_eq!(config[0]["currentValue"], "bypassPermissions");
    assert_eq!(fake.texts(), ["plan:exit-plan-bypass"]);
}

#[test]
fn unit_fake_scripts_plan_rejected_by_claude_ends_the_turn() {
    let mut fake = Fake::start(CLAUDE);
    fake.answers.push_back(selected("reject"));
    let response = fake.prompt("plan_confirm; say after");
    assert_eq!(stop(&response), "cancelled");
    assert_eq!(fake.texts(), ["plan:reject"]);
}

#[test]
fn unit_fake_scripts_plan_confirm_codex_goes_idle_first() {
    let mut fake = Fake::start(CODEX);
    fake.set_config("collaboration_mode", "plan");
    fake.answers.push_back(selected("implement_plan"));
    let response = fake.prompt("plan_confirm");
    let asked = fake.index(request("session/request_permission"));
    assert!(fake.index(|m| status(m)["type"] == "idle") < asked, "K12");
    let options = ids(&fake.seen[asked]["params"]["options"], "optionId");
    assert_eq!(options, ["implement_plan", "revise_plan"]);
    let config = fake.updates("config_option_update")[0]["configOptions"].clone();
    assert_eq!(config[1]["currentValue"], "default");
    assert_eq!(fake.texts(), ["plan:implement_plan"]);
    assert_eq!(stop(&response), "end_turn");
}

#[test]
fn unit_fake_scripts_form_per_persona() {
    let claude = [
        "question_0",
        "question_0_custom",
        "question_1",
        "question_1_custom",
    ];
    for (args, keys) in [
        (CLAUDE, claude),
        (CODEX, ["color", "color_note", "size", "size_note"]),
    ] {
        let mut fake = Fake::start(args);
        let content = json!({keys[0]: "red", keys[2]: "s"});
        fake.answers
            .push_back(json!({"action": "accept", "content": content}));
        fake.prompt("form color=red|blue size=s|m");
        let schema = fake.find(request("elicitation/create"))["params"]["requestedSchema"].clone();
        let shown: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        assert_eq!(shown, keys);
        assert_eq!(
            ids(&schema["properties"][keys[0]]["oneOf"], "const"),
            ["red", "blue"]
        );
        assert_eq!(fake.texts(), [format!("form:{content}")]);
        if args == CODEX {
            assert_eq!(schema["required"], json!(["color", "size"]));
            fake.index(|m| status(m)["activeFlags"] == json!(["waitingOnUserInput"]));
        }
    }
}

#[test]
fn unit_fake_scripts_steering_as_each_persona() {
    let prompt_required = json!({"steering": {"idleBehavior": "promptRequired"}});
    for (args, idle) in [(CLAUDE, "promptRequired"), (CODEX, "startedNewTurn")] {
        let mut fake = Fake::start(args);
        let prompt = fake.start_prompt("say waiting; wait_cancel");
        fake.wait_for("turn", says("waiting"));
        let id = fake.steer("also this", Value::Null);
        assert_eq!(fake.response(id)["result"]["outcome"], "injected");
        fake.wait_for("steered", says("steered:also this"));
        fake.cancel();
        fake.response(prompt);
        let id = fake.steer("say idle turn", prompt_required.clone());
        assert_eq!(fake.response(id)["result"]["outcome"], idle);
        let id = fake.steer("say new turn", Value::Null);
        assert_eq!(fake.response(id)["result"]["outcome"], "startedNewTurn");
        // The steer text runs as a turn of its own.
        fake.wait_for("new turn", says("new turn"));
    }
}

#[test]
fn unit_fake_scripts_steer_commands() {
    let mut fake = Fake::start(CLAUDE);
    for (script, outcome) in [
        ("steer_ok", "injected"),
        ("steer_new_turn", "startedNewTurn"),
    ] {
        fake.prompt(script);
        let id = fake.steer("say x", Value::Null);
        assert_eq!(fake.response(id)["result"]["outcome"], outcome);
    }
    fake.prompt("steer_hang");
    let id = fake.steer("z", Value::Null);
    assert!(fake.quiet(300, |m| m["id"] == id));
}

#[test]
fn unit_fake_scripts_hang_until_close_and_resume() {
    let mut fake = Fake::start(CLAUDE);
    let prompt = fake.start_prompt("say stuck; hang");
    fake.wait_for("stuck", says("stuck"));
    fake.cancel();
    assert!(fake.quiet(300, |m| m["id"] == prompt));
    // K1: close, then resume, then a new prompt.
    let close = fake.call("session/close", json!({"sessionId": fake.session}));
    assert_eq!(close["result"], json!({}));
    fake.resume();
    assert_eq!(stop(&fake.prompt("say again")), "end_turn");
}

#[test]
fn unit_fake_scripts_calls_record_new_resume_close_and_prompts() {
    let path = std::env::temp_dir().join(format!("cupper-fake-calls-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let mut fake = Fake::start(&["--calls", path.to_str().unwrap()]);
    fake.prompt("say one");
    fake.call("session/close", json!({"sessionId": fake.session}));
    fake.resume();
    let calls = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(&path);
    let calls: Vec<Value> = calls
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let methods: Vec<&str> = calls.iter().filter_map(|c| c["method"].as_str()).collect();
    assert_eq!(
        methods,
        [
            "initialize",
            "session/new",
            "session/prompt",
            "session/close",
            "session/resume"
        ]
    );
    assert_eq!(calls[2]["text"], "say one");
    assert_eq!(calls[4]["session"], json!(fake.session));
}

#[test]
fn unit_fake_scripts_late_tool_after_cancel() {
    let mut fake = Fake::start(CODEX);
    let prompt = fake.start_prompt("late_tool t9 100");
    fake.wait_for("command", is("tool_call"));
    fake.cancel();
    assert_eq!(stop(&fake.response(prompt)), "cancelled");
    let end = fake.wait_for("late end", |m| update(m)["status"] == "completed");
    assert_eq!(update(&end)["toolCallId"], "t9");
    let output = fake.index(|m| update(m)["_meta"]["terminal_output"]["data"] == "late output\n");
    assert!(output > fake.index(|m| m["id"] == prompt));
}

#[test]
fn unit_fake_scripts_autonomous_runs_after_the_prompt() {
    let mut fake = Fake::start(CLAUDE);
    let response = fake.prompt("say first; autonomous 50; sleep 10; say later");
    assert_eq!(stop(&response), "end_turn");
    fake.wait_for("later", says("later"));
    assert_eq!(fake.texts(), ["first", "later"]);
}

#[test]
fn unit_fake_scripts_crash_cli_until_resume() {
    for script in ["crash_cli", "output_then_crash"] {
        let mut fake = Fake::start(CLAUDE);
        let error = fake.prompt(script)["error"].clone();
        assert_eq!(error["code"], -32603);
        assert!(
            error["message"]
                .as_str()
                .unwrap()
                .contains("exited unexpectedly")
        );
        assert_eq!(
            fake.texts().len(),
            usize::from(script == "output_then_crash")
        );
        let not_found = json!({"details": "Session not found"});
        assert_eq!(fake.prompt("say x")["error"]["data"], not_found);
        assert_eq!(fake.set_config("mode", "plan")["error"]["data"], not_found);
        fake.resume();
        assert_eq!(stop(&fake.prompt("say back")), "end_turn");
    }
}

#[test]
fn unit_fake_scripts_restartable() {
    let mut fake = Fake::start(CODEX);
    let error = fake.prompt("restartable")["error"].clone();
    assert_eq!(
        [&error["code"], &error["data"]["restartable"]],
        [&json!(1001), &json!(true)]
    );
    assert_eq!(stop(&fake.prompt("say ok")), "end_turn");
}

#[test]
fn unit_fake_scripts_upstream_retry_and_thread_status() {
    let mut fake = Fake::start(CODEX);
    fake.prompt(
        "upstream_retry 2; thread_status active waitingOnUserInput; thread_status systemError",
    );
    let codex: Vec<Value> = fake
        .updates("session_info_update")
        .iter()
        .map(|u| u["_meta"]["codex"].clone())
        .collect();
    let retry = &codex[2]["error"];
    assert_eq!(
        [&retry["message"], &retry["willRetry"]],
        [&json!("Reconnecting... 2/5"), &json!(true)]
    );
    let waiting = json!({"type": "active", "activeFlags": ["waitingOnUserInput"]});
    assert_eq!(
        [&codex[3]["threadStatus"], &codex[4]["threadStatus"]],
        [&waiting, &json!({"type": "systemError"})]
    );
}

#[test]
fn unit_fake_scripts_echo_env_shows_names_only() {
    let mut fake = Fake::spawn_with(CLAUDE, &[("CUPPER_FAKE_TEST_SECRET", "s3cr3t-value")]);
    fake.initialize();
    fake.open(Value::Null);
    fake.prompt("echo_env");
    let text = &fake.texts()[0];
    assert!(
        text.starts_with(&format!("cwd={} clientInfo=", fake.cwd.display())),
        "{text}"
    );
    assert!(text.contains(r#""name":"cupper""#), "{text}");
    assert!(
        text.contains("CUPPER_FAKE_TEST_SECRET") && !text.contains("s3cr3t-value"),
        "{text}"
    );
}

#[test]
fn unit_fake_scripts_auth_none() {
    let mut fake = Fake::start(CLAUDE);
    assert_eq!(fake.prompt("auth_none")["error"]["code"], -32000);
    let status = fake
        .seen
        .iter()
        .rev()
        .find(|m| m["method"] == "_auth/status_update");
    assert_eq!(status.unwrap()["params"]["authStatus"]["kind"], "none");
}

// Raw lines, `_meta` echo and the global invariant

#[test]
fn unit_fake_scripts_raw_line_is_verbatim_and_in_order() {
    let mut fake = Fake::start(CLAUDE);
    let raw = r#"{"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "${session}",
        "update": {"sessionUpdate": "tool_call", "toolCallId": "r1", "title": "a;b", "kind": "teleport"}}}"#
        .replace('\n', "");
    fake.prompt(&format!("say one; raw {raw} ; say two"));
    let line = raw.replace("${session}", &fake.session);
    let at = fake
        .raw
        .iter()
        .position(|l| *l == line)
        .expect("the raw line, verbatim");
    assert!(fake.index(says("one")) < at && at < fake.index(says("two")));
    fake.prompt("raw {not json");
    assert_eq!(fake.texts().last().unwrap(), "fake reply to: raw {not json");
}

#[test]
fn unit_fake_scripts_echo_meta_shows_the_session_meta() {
    let mut fake = Fake::spawn(CLAUDE);
    fake.initialize();
    let meta = json!({"claudeCode": {"options": {"settingSources": ["user"]}}});
    fake.open(meta.clone());
    fake.prompt("echo_meta");
    assert_eq!(fake.texts(), [format!("meta={meta}")]);
}

#[test]
fn unit_fake_scripts_second_prompt_in_flight_exits() {
    let mut fake = Fake::start(CLAUDE);
    // A prompt sent right after the previous answer is not a second one.
    for _ in 0..3 {
        fake.prompt("say quick");
    }
    fake.start_prompt("wait_cancel");
    fake.start_prompt("say second");
    let (code, stderr) = fake.exit();
    assert_eq!(code, Some(EXIT_SECOND_PROMPT));
    assert!(stderr.contains("second prompt"), "{stderr}");
}
