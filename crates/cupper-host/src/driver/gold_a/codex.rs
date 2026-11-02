//! Codex's A-layer cases: a golden per scenario of SP-1's recordings of
//! codex-acp 2.2.2 (CLI 0.159.2 and 0.160.1), the tool fields, the values the
//! tables must have, and text errors (K17).

use cupper_protocol::recording::Recording;
use serde_json::{Value, json};

use super::{CODEX, extract, gold_a, match_up, not_in_tables, recordings, tool_calls};

gold_a! { CODEX, SCENARIOS;
    gold_a_codex_s01 => "s01-init-session",
    gold_a_codex_s02 => "s02-normal-turn",
    gold_a_codex_s02b => "s02b-plan",
    gold_a_codex_s03 => "s03-exec",
    gold_a_codex_s04 => "s04-approval",
    gold_a_codex_s04b => "s04b-reject-command",
    gold_a_codex_s05 => "s05-questions",
    gold_a_codex_s06 => "s06-queue-steer",
    gold_a_codex_s07 => "s07-cancel",
    gold_a_codex_s07b => "s07b-cli-crash-recovery",
    gold_a_codex_s08 => "s08-list",
    gold_a_codex_s09 => "s09-load-resume",
    gold_a_codex_s10 => "s10-unauthenticated",
    gold_a_codex_s10x => "s10x-model-unsupported",
    gold_a_codex_s11 => "s11-image",
    gold_a_codex_s12 => "s12-resources",
}

#[test]
fn gold_a_codex_recordings_and_goldens_match_up() {
    match_up(&CODEX, SCENARIOS);
}

/// The start of an inline recording of Codex's.
const HEADER: &str = r#"{"format":"cupper-rec","v":1,"source":"host","adapter":"codex-acp","adapter_version":"2.2.2","cli":"codex","cli_version":"0.159.2","scenario":"inline","usage":["gold-a"]}
{"seq":1,"t":0,"instance":0,"dir":"note","note":{"event":"instance_started"}}"#;

fn inline(frames: &str) -> Recording {
    Recording::parse(&format!("{HEADER}\n{frames}")).expect("a recording")
}

/// Commands: a read has its output as text and its exit code from
/// `rawOutput`; a shell command its title as the command, its terminal's cwd,
/// its output streamed and its exit code (s03). Edits: their files and a diff
/// each (s04); `apply_patch` in the shell has none (K24, s02). No update comes
/// after a call's end, so the judge never takes one for running again (T058).
#[test]
fn gold_a_tool_fields_codex_of_commands_and_edits() {
    let calls = tool_calls(&CODEX, "s03-exec");
    let keys = [
        "tool_kind",
        "command",
        "cwd",
        "output",
        "exit_code",
        "status",
    ];
    let fields = |c: &Value| Value::from(keys.map(|key| c[key].clone()).to_vec());
    let cwd = "<tmp>/repos/codex-local";
    let sh = "echo out; echo err 1>&2; exit 3";
    assert_eq!(
        calls.iter().map(fields).collect::<Vec<_>>()[..2],
        [
            json!(["read", null, null, {"replace": "README.md\napp.js\nhello.txt\n"}, 0, "completed"]),
            json!(["execute", sh, cwd, {"append": "out\nerr\n"}, 3, "failed"]),
        ]
    );
    let edits: Vec<Value> = (tool_calls(&CODEX, "s04-approval").iter())
        .map(|c| json!([c["paths"], c["diffs"][0]["change"], c["diffs"][0]["stat"]]))
        .collect();
    let stat = json!({"added": 1, "removed": 0});
    assert_eq!(
        edits[..2],
        [
            json!([["a.txt"], "add", stat]),
            json!([["b.txt"], "add", stat])
        ]
    );
    let patches = tool_calls(&CODEX, "s02-normal-turn");
    let patch = |c: &&Value| {
        c["command"]
            .as_str()
            .is_some_and(|c| c.starts_with("apply_patch"))
    };
    assert!(patches.iter().filter(patch).all(|c| c["diffs"].is_null()));
    assert!(patches.iter().any(|c| patch(&c)));

    for (name, recording) in recordings(&CODEX, None) {
        let mut ended: Vec<Value> = Vec::new();
        for value in extract(&recording) {
            let Some(update) = value.pointer("/event/update/tool_update") else {
                continue;
            };
            let id = &update["tool_call_id"];
            assert!(!ended.contains(id), "{name:?}: {id} updated after its end");
            if update["status"] == "completed" || update["status"] == "failed" {
                ended.push(id.clone());
            }
        }
    }
}

#[test]
fn gold_a_option_map_codex_has_every_recorded_value() {
    for (name, recording) in recordings(&CODEX, None) {
        assert_eq!(not_in_tables(&recording), Vec::<String>::new(), "{name:?}");
    }
}

/// A mode, an option and a tool kind the tables do not have are all flagged;
/// so are options Codex has but the recordings never showed (`decline`, and
/// `allow_for_session` on a command).
#[test]
fn gold_a_option_map_codex_flags_values_outside_the_tables() {
    let recording = inline(
        r#"{"seq":2,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":1,"result":{"sessionId":"s","configOptions":[{"id":"mode","name":"Mode","category":"mode","type":"select","currentValue":"agent","options":[{"value":"agent","name":"Auto review"},{"value":"agent-yolo","name":"Yolo"}]}]}}}
{"seq":3,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"tool_call","toolCallId":"t","title":"x","kind":"teleport"}}}}
{"seq":4,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":7,"method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"c","kind":"execute"},"options":[{"optionId":"allow_once","name":"Yes","kind":"allow_once"},{"optionId":"allow_for_session","name":"Session","kind":"allow_always"},{"optionId":"decline","name":"No","kind":"reject_once"}]}}}"#,
    );
    assert_eq!(
        not_in_tables(&recording),
        [
            "option allow_for_session in Command",
            "option decline in Command",
            "permission mode agent-yolo",
            "tool kind teleport"
        ]
    );
}

/// K17: after `systemError`, the adapter's text without a `messageId` is the
/// turn's error and the prompt fails with it (s10x). The model's text after
/// it, which has an ID, stays text; so does text without an ID once the
/// thread is active again.
#[test]
fn gold_a_codex_s10x_text_errors_fail_the_prompt() {
    let (_, s10x) = recordings(&CODEX, Some("s10x-model-unsupported")).remove(0);
    let out = extract(&s10x);
    let failed = out
        .iter()
        .find_map(|v| v.pointer("/event/prompt_failed/error"));
    let message =
        "The 'gpt-5.4-mini' model is not supported when using Codex with a ChatGPT account.";
    let error = json!({"class": "model_unsupported", "form": "text", "message": message});
    assert_eq!(failed, Some(&error));
    assert!(
        !out.iter()
            .any(|v| v.pointer("/event/update/agent_text").is_some())
    );

    let recording = inline(
        r#"{"seq":2,"t":0,"instance":0,"dir":"send","msg":{"jsonrpc":"2.0","id":2,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}}}
{"seq":3,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","_meta":{"codex":{"threadStatus":{"type":"active","activeFlags":[]}}}}}}}
{"seq":4,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","_meta":{"codex":{"threadStatus":{"type":"systemError"}}}}}}}
{"seq":5,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"boom\n\n"}}}}}
{"seq":6,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","messageId":"msg_1","content":{"type":"text","text":"model text"}}}}}
{"seq":7,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":2,"result":{"stopReason":"end_turn"}}}
{"seq":8,"t":0,"instance":0,"dir":"send","msg":{"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"s","prompt":[]}}}
{"seq":9,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"session_info_update","_meta":{"codex":{"threadStatus":{"type":"active","activeFlags":[]}}}}}}}
{"seq":10,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"s","update":{"sessionUpdate":"agent_message_chunk","content":{"type":"text","text":"Warning: slow"}}}}}
{"seq":11,"t":0,"instance":0,"dir":"recv","msg":{"jsonrpc":"2.0","id":3,"result":{"stopReason":"end_turn"}}}"#,
    );
    let texts: Vec<Value> = (extract(&recording).into_iter())
        .filter_map(|v| {
            (v.pointer("/event/update/agent_text/text").cloned())
                .or(v.pointer("/event/prompt_failed/error/message").cloned())
                .or(v.pointer("/event/prompt_done/stop_reason").cloned())
        })
        .collect();
    assert_eq!(texts, ["model text", "boom", "Warning: slow", "end_turn"]);
}
