//! T003 Q9 probe: does the ACP Rust SDK `=3.3.0`, used as the client of a real
//! claude-agent-acp, (1) send the `_session/steering` extension request and
//! get its response, (2) receive the `_auth/status_update` extension
//! notification, (3) receive `$/cancel_request` for a pending permission
//! request? It also checks (4) that dropping an unanswered `session/prompt`
//! makes the SDK send `$/cancel_request`, which the adapter turns into a
//! cancel of the turn.
//!
//! A line tap sits between the adapter's stdio and the SDK (as the host's wire
//! tap will, design 5.6.10) and records every frame, so what the SDK handlers
//! saw can be compared with what crossed the wire.
//!
//! Usage: q9-probe <adapter entry .js> <cwd: throwaway repo> <frames out.jsonl> <findings out.json>
//! Environment: CLAUDE_CODE_EXECUTABLE (absolute path of the claude binary).
//!
//! Safety (brief T003): haiku, lowest effort, permission mode set explicitly
//! to `default`, only `allow_once` is ever selected, prompts are fixed and do
//! not fetch anything from the network.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    CancelNotification, CancelRequestNotification, ClientCapabilities, CloseSessionRequest, ContentBlock,
    CreateElicitationRequest, CreateElicitationResponse, ElicitationAction, ElicitationCapabilities,
    ElicitationFormCapabilities, Implementation, InitializeRequest, NewSessionRequest, PermissionOptionKind,
    PromptRequest, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionConfigKind, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOptions, SessionNotification, SessionUpdate, SetSessionConfigOptionRequest, TextContent,
};
use agent_client_protocol::{
    Agent, ByteStreams, Client, ConnectionTo, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, Responder,
};
use anyhow::{Context as _, anyhow, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

// ---------------------------------------------------------------------------
// The two extension messages, declared with the SDK's derive macros

/// claude-agent-acp's steering request (`_meta.steering.supported` in `initialize`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_session/steering", response = SteeringResponse)]
#[serde(rename_all = "camelCase")]
struct SteeringRequest {
    session_id: String,
    prompt: Vec<ContentBlock>,
    #[serde(rename = "_meta", skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
struct SteeringResponse {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
}

/// Login state pushed by the adapter. Only `kind` and the plan are kept.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "_auth/status_update")]
#[serde(rename_all = "camelCase")]
struct AuthStatusUpdate {
    auth_status: Value,
}

// ---------------------------------------------------------------------------
// Frame log in SP-1's recording format: {seq, t, dir, msg | line | note, data}

struct FrameLog {
    start: Instant,
    seq: AtomicU64,
    file: Mutex<std::fs::File>,
    lines: Mutex<Vec<(String, Value)>>,
}

impl FrameLog {
    fn t(&self) -> u128 {
        self.start.elapsed().as_millis()
    }

    fn put(&self, mut v: Value) {
        use std::io::Write;
        v["seq"] = json!(self.seq.fetch_add(1, Ordering::SeqCst) + 1);
        v["t"] = json!(self.t());
        let mut f = self.file.lock().unwrap();
        let _ = writeln!(f, "{v}");
    }

    fn frame(&self, dir: &str, line: &str) {
        match serde_json::from_str::<Value>(line) {
            Ok(msg) => {
                self.lines.lock().unwrap().push((dir.to_owned(), msg.clone()));
                self.put(json!({ "dir": dir, "msg": msg }));
            }
            Err(_) => self.put(json!({ "dir": format!("{dir}-nonjson"), "line": line })),
        }
    }

    fn note(&self, note: &str, data: Value) {
        self.put(json!({ "dir": "note", "note": note, "data": data }));
    }

    /// Frames seen on the wire so far, as (direction, message).
    fn seen(&self) -> Vec<(String, Value)> {
        self.lines.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------
// What the SDK handlers report to the probe's main task

#[derive(Debug)]
enum Ev {
    Update {
        session: String,
        kind: String,
        text: Option<String>,
    },
    Auth {
        kind: String,
    },
    CancelRequest {
        request_id: Value,
    },
    Permission {
        held: bool,
    },
    MarkerFired,
}

fn kind_of(update: &SessionUpdate) -> String {
    serde_json::to_value(update)
        .ok()
        .and_then(|v| v.get("sessionUpdate").and_then(|k| k.as_str()).map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

type HeldResponder = Arc<Mutex<Option<Responder<RequestPermissionResponse>>>>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let [_, entry, cwd, frames_out, findings_out] = args.as_slice() else {
        bail!("usage: q9-probe <adapter entry .js> <cwd> <frames out.jsonl> <findings out.json>");
    };
    let cwd = PathBuf::from(cwd);

    // Same allowlist as SP-1 and the SP-4a driver; nothing of the caller's session leaks in.
    let mut cmd = Command::new(std::env::var("T003_NODE").unwrap_or_else(|_| "node".into()));
    cmd.arg(entry).env_clear();
    for k in [
        "HOME",
        "PATH",
        "SHELL",
        "TMPDIR",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "USER",
        "LOGNAME",
        "CLAUDE_CODE_EXECUTABLE",
    ] {
        if let Ok(v) = std::env::var(k) {
            cmd.env(k, v);
        }
    }
    cmd.stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    cmd.kill_on_drop(true);
    let mut child = cmd.spawn().context("spawn adapter")?;
    let mut child_stdin = child.stdin.take().unwrap();
    let child_stdout = child.stdout.take().unwrap();

    let log = Arc::new(FrameLog {
        start: Instant::now(),
        seq: AtomicU64::new(0),
        file: Mutex::new(std::fs::File::create(frames_out)?),
        lines: Mutex::new(Vec::new()),
    });
    log.note(
        "header",
        json!({ "scenario": "q9-probe", "sdk": "agent-client-protocol =3.3.0" }),
    );

    // Wire tap: adapter stdout -> log -> SDK, SDK -> log -> adapter stdin.
    let (sdk_side, tap_side) = tokio::io::duplex(1 << 20);
    let (tap_read, mut tap_write) = tokio::io::split(tap_side);
    let tap_log = log.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(child_stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tap_log.frame("recv", &line);
            if line.contains("\"_auth/status_update\"") || line.contains("\"$/cancel_request\"") {
                tap_log.note("tap-saw-extension", json!({ "line_has": if line.contains("cancel_request") { "$/cancel_request" } else { "_auth/status_update" } }));
            }
            if tap_write.write_all(line.as_bytes()).await.is_err() || tap_write.write_all(b"\n").await.is_err() {
                break;
            }
        }
    });
    let tap_log = log.clone();
    tokio::spawn(async move {
        let mut lines = BufReader::new(tap_read).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tap_log.frame("send", &line);
            if child_stdin.write_all(line.as_bytes()).await.is_err() || child_stdin.write_all(b"\n").await.is_err() {
                break;
            }
        }
    });
    let (sdk_read, sdk_write) = tokio::io::split(sdk_side);
    let transport = ByteStreams::new(sdk_write.compat_write(), sdk_read.compat());

    let (ev_tx, ev_rx) = mpsc::unbounded_channel::<Ev>();
    let hold = Arc::new(AtomicBool::new(false));
    let held: HeldResponder = Arc::new(Mutex::new(None));

    let (tx_update, tx_auth, tx_cancel, tx_perm) = (ev_tx.clone(), ev_tx.clone(), ev_tx.clone(), ev_tx.clone());
    let (hold_h, held_h) = (hold.clone(), held.clone());
    let main_log = log.clone();
    let findings = Client
        .builder()
        .name("cupper-t003-probe")
        .on_receive_notification(
            async move |n: SessionNotification, _cx| {
                let text = match &n.update {
                    SessionUpdate::AgentMessageChunk(c) => match &c.content {
                        ContentBlock::Text(t) => Some(t.text.clone()),
                        _ => None,
                    },
                    _ => None,
                };
                let _ = tx_update.send(Ev::Update {
                    session: n.session_id.to_string(),
                    kind: kind_of(&n.update),
                    text,
                });
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_notification(
            async move |n: AuthStatusUpdate, _cx| {
                let kind = n
                    .auth_status
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .unwrap_or("?")
                    .to_owned();
                let _ = tx_auth.send(Ev::Auth { kind });
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_notification(
            async move |n: CancelRequestNotification, _cx| {
                let _ = tx_cancel.send(Ev::CancelRequest {
                    request_id: serde_json::to_value(&n.request_id).unwrap_or(Value::Null),
                });
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |req: RequestPermissionRequest, responder: Responder<RequestPermissionResponse>, _cx| {
                if hold_h.swap(false, Ordering::SeqCst) {
                    // Keep the request open; report when the SDK marks it cancelled.
                    let marker = responder.cancellation();
                    *held_h.lock().unwrap() = Some(responder);
                    let tx = tx_perm.clone();
                    tokio::spawn(async move {
                        marker.cancelled().await;
                        let _ = tx.send(Ev::MarkerFired);
                    });
                    let _ = tx_perm.send(Ev::Permission { held: true });
                    return Ok(());
                }
                // Only ever `allow_once`; nothing persistent.
                let outcome = match req.options.iter().find(|o| o.kind == PermissionOptionKind::AllowOnce) {
                    Some(o) => RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(o.option_id.clone())),
                    None => RequestPermissionOutcome::Cancelled,
                };
                let _ = tx_perm.send(Ev::Permission { held: false });
                responder.respond(RequestPermissionResponse::new(outcome))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: CreateElicitationRequest, responder: Responder<CreateElicitationResponse>, _cx| {
                responder.respond(CreateElicitationResponse::new(ElicitationAction::Cancel))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, async move |cx: ConnectionTo<Agent>| {
            let r = run(cx, ev_rx, hold, held, main_log, cwd).await;
            Ok(r)
        })
        .await
        .map_err(|e| anyhow!("connection: {e}"))?;

    let findings = match findings {
        Ok(f) => f,
        Err(e) => json!({ "error": e.to_string() }),
    };
    std::fs::write(findings_out, serde_json::to_string_pretty(&findings)?)?;
    println!("{}", serde_json::to_string_pretty(&findings)?);
    let _ = tokio::time::timeout(Duration::from_secs(10), child.wait()).await;
    Ok(())
}

// ---------------------------------------------------------------------------

struct Events {
    rx: mpsc::UnboundedReceiver<Ev>,
}

impl Events {
    /// Next event matching `pred` within `ms`; other events are dropped.
    async fn wait(&mut self, ms: u64, mut pred: impl FnMut(&Ev) -> bool) -> Option<Ev> {
        let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
        loop {
            match tokio::time::timeout_at(deadline, self.rx.recv()).await {
                Ok(Some(ev)) if pred(&ev) => return Some(ev),
                Ok(Some(_)) => continue,
                _ => return None,
            }
        }
    }

    /// Drain everything already queued, collecting assistant text for `session`.
    fn drain_text(&mut self, session: &str, into: &mut String) {
        while let Ok(ev) = self.rx.try_recv() {
            if let Ev::Update {
                session: s,
                text: Some(t),
                ..
            } = ev
                && s == session
            {
                into.push_str(&t);
            }
        }
    }
}

fn select(
    options: &[SessionConfigOption],
    category: SessionConfigOptionCategory,
    pick: impl Fn(&str) -> bool,
) -> Option<(String, String, String)> {
    options.iter().find_map(|o| {
        let SessionConfigKind::Select(sel) = &o.kind else {
            return None;
        };
        if o.category.as_ref() != Some(&category) {
            return None;
        }
        let SessionConfigSelectOptions::Ungrouped(opts) = &sel.options else {
            return None;
        };
        opts.iter()
            .find(|v| pick(&v.value.to_string()))
            .map(|v| (o.id.to_string(), v.value.to_string(), sel.current_value.to_string()))
    })
}

fn current(options: &[SessionConfigOption], category: SessionConfigOptionCategory) -> Option<String> {
    options.iter().find_map(|o| match &o.kind {
        SessionConfigKind::Select(sel) if o.category.as_ref() == Some(&category) => Some(sel.current_value.to_string()),
        _ => None,
    })
}

async fn set(
    cx: &ConnectionTo<Agent>,
    session: &str,
    id: String,
    value: String,
) -> anyhow::Result<Vec<SessionConfigOption>> {
    let resp = cx
        .send_request(SetSessionConfigOptionRequest::new(
            session.to_owned(),
            id,
            value.as_str(),
        ))
        .block_task()
        .await?;
    Ok(resp.config_options)
}

/// Throws unless the session is in `default` mode on haiku.
fn assert_safe(options: &[SessionConfigOption]) -> anyhow::Result<()> {
    let mode = current(options, SessionConfigOptionCategory::Mode);
    let model = current(options, SessionConfigOptionCategory::Model);
    if mode.as_deref() == Some("default") && model.as_deref().is_some_and(|m| m.to_lowercase().contains("haiku")) {
        Ok(())
    } else {
        bail!("refusing to prompt: mode={mode:?} model={model:?}")
    }
}

fn text(s: &str) -> Vec<ContentBlock> {
    vec![ContentBlock::Text(TextContent::new(s.to_owned()))]
}

async fn run(
    cx: ConnectionTo<Agent>,
    rx: mpsc::UnboundedReceiver<Ev>,
    hold: Arc<AtomicBool>,
    held: HeldResponder,
    log: Arc<FrameLog>,
    cwd: PathBuf,
) -> anyhow::Result<Value> {
    let mut ev = Events { rx };
    let mut out = json!({});

    // 1. initialize
    let caps =
        ClientCapabilities::new().elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()));
    let init = cx
        .send_request(
            InitializeRequest::new(ProtocolVersion::V1)
                .client_capabilities(caps)
                .client_info(Implementation::new("cupper-t003-probe", "0.0.0")),
        )
        .block_task()
        .await?;
    // The steering flag is in the response's top-level `_meta`, not under agentCapabilities.
    let init_json = serde_json::to_value(&init)?;
    let wire_init = log
        .seen()
        .into_iter()
        .find(|(d, m)| d == "recv" && m.pointer("/result/agentCapabilities").is_some())
        .map(|(_, m)| m["result"].clone())
        .unwrap_or_default();
    out["initialize"] = json!({
        "steeringSupportedOnWire": wire_init.pointer("/_meta/steering/supported"),
        "steeringSupportedAfterSdkParse": init_json.pointer("/_meta/steering/supported"),
        "promptQueueingAfterSdkParse": init_json.pointer("/agentCapabilities/_meta/claudeCode/promptQueueing"),
    });

    // 2. _auth/status_update through a typed SDK handler
    let auth = ev.wait(5000, |e| matches!(e, Ev::Auth { .. })).await;
    out["authStatusUpdate"] = json!({
        "receivedBySdkHandler": auth.is_some(),
        "kind": match &auth { Some(Ev::Auth { kind }) => json!(kind), _ => Value::Null },
        "onWire": log.seen().iter().any(|(d, m)| d == "recv" && m["method"] == "_auth/status_update"),
    });

    // 3. session in default mode on haiku, lowest effort
    let resp = cx
        .send_request(NewSessionRequest::new(cwd.clone()))
        .block_task()
        .await?;
    let sid = resp.session_id.to_string();
    let mut options = resp.config_options.unwrap_or_default();
    if let Some((id, value, cur)) = select(&options, SessionConfigOptionCategory::Model, |v| {
        v.to_lowercase().contains("haiku")
    }) && value != cur
    {
        options = set(&cx, &sid, id, value).await?;
    }
    if let Some((id, value, cur)) = select(&options, SessionConfigOptionCategory::ThoughtLevel, |v| v == "low")
        && value != cur
    {
        options = set(&cx, &sid, id, value).await?;
    }
    let (mode_id, mode_value, _) = select(&options, SessionConfigOptionCategory::Mode, |v| v == "default")
        .ok_or_else(|| anyhow!("no default mode"))?;
    options = set(&cx, &sid, mode_id, mode_value).await?;
    assert_safe(&options)?;

    // 4. _session/steering while idle: no turn starts, no model call
    let idle = cx
        .send_request(SteeringRequest {
            session_id: sid.clone(),
            prompt: text("Reply with exactly: idle-steer"),
            meta: Some(json!({ "steering": { "idleBehavior": "promptRequired" } })),
        })
        .block_task()
        .await;
    out["steeringIdle"] = match idle {
        Ok(r) => json!({ "ok": true, "outcome": r.outcome, "reason": r.reason }),
        Err(e) => json!({ "ok": false, "error": e.to_string() }),
    };

    // 5. _session/steering during a running text turn
    assert_safe(&options)?;
    let prompt_cx = cx.clone();
    let prompt_sid = sid.clone();
    let turn = tokio::spawn(async move {
        prompt_cx
            .send_request(PromptRequest::new(
                prompt_sid,
                text("Count from 1 to 30, one number per line, then write the word FINISHED on its own line."),
            ))
            .block_task()
            .await
    });
    let first = ev
        .wait(
            60_000,
            |e| matches!(e, Ev::Update { kind, .. } if kind == "agent_message_chunk"),
        )
        .await;
    let steer = cx
        .send_request(SteeringRequest {
            session_id: sid.clone(),
            prompt: text("Also write the word STEERED on its own line at the very end."),
            meta: None,
        })
        .block_task()
        .await;
    let turn_result = turn.await?;
    let mut reply = String::new();
    ev.drain_text(&sid, &mut reply);
    out["steeringInTurn"] = json!({
        "sentAfterFirstChunk": first.is_some(),
        "response": match &steer { Ok(r) => json!({ "outcome": r.outcome, "reason": r.reason }), Err(e) => json!({ "error": e.to_string() }) },
        "turnStopReason": match &turn_result { Ok(r) => serde_json::to_value(r.stop_reason)?, Err(e) => json!(e.to_string()) },
        "replyMentionsSteered": reply.contains("STEERED"),
    });

    // 6. $/cancel_request for a pending permission request
    assert_safe(&options)?;
    hold.store(true, Ordering::SeqCst);
    let prompt_cx = cx.clone();
    let prompt_sid = sid.clone();
    let turn = tokio::spawn(async move {
        prompt_cx
            .send_request(PromptRequest::new(
                prompt_sid,
                text("Use your Write tool to create a file named q9.txt in the current directory whose entire content is the single line q9-held. Then reply with exactly: done"),
            ))
            .block_task()
            .await
    });
    let asked = ev.wait(90_000, |e| matches!(e, Ev::Permission { held: true })).await;
    let perm_id = log
        .seen()
        .iter()
        .rev()
        .find(|(d, m)| d == "recv" && m["method"] == "session/request_permission")
        .map(|(_, m)| m["id"].clone());
    tokio::time::sleep(Duration::from_millis(1000)).await;
    cx.send_notification(CancelNotification::new(sid.clone()))?;
    let t_cancel = log.t();
    let mut raw_cancel = None;
    let mut marker = false;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    while (raw_cancel.is_none() || !marker) && tokio::time::Instant::now() < deadline {
        match ev
            .wait(500, |e| matches!(e, Ev::CancelRequest { .. } | Ev::MarkerFired))
            .await
        {
            Some(Ev::CancelRequest { request_id }) => raw_cancel = Some(request_id),
            Some(Ev::MarkerFired) => marker = true,
            _ => {}
        }
    }
    let turn_result = tokio::time::timeout(Duration::from_secs(30), turn).await;
    let late_answer = held
        .lock()
        .unwrap()
        .take()
        .map(|r| r.respond(RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)));
    out["cancelRequest"] = json!({
        "permissionAsked": asked.is_some(),
        "permissionRequestId": perm_id,
        "rawHandlerSaw": raw_cancel.is_some(),
        "rawHandlerRequestId": raw_cancel,
        "requestIdMatchesPermission": raw_cancel.is_some() && raw_cancel == perm_id,
        "responderMarkerFired": marker,
        "onWire": log.seen().iter().any(|(d, m)| d == "recv" && m["method"] == "$/cancel_request"),
        "msFromSessionCancel": log.t() - t_cancel,
        "turnStopReason": match turn_result { Ok(Ok(Ok(r))) => serde_json::to_value(r.stop_reason)?, Ok(Ok(Err(e))) => json!(e.to_string()), _ => json!("no result within 30 s") },
        "lateCancelledAnswerAccepted": late_answer.map(|r| r.is_ok()),
        "fileCreated": cwd.join("q9.txt").exists(),
    });

    // 7. Dropping an unanswered prompt request: the SDK sends $/cancel_request
    //    and the adapter cancels the turn.
    assert_safe(&options)?;
    let sent = cx.send_request(PromptRequest::new(
        sid.clone(),
        text("Count from 1 to 40, one number per line, then write the word FINISHED on its own line."),
    ));
    let first = ev
        .wait(
            60_000,
            |e| matches!(e, Ev::Update { kind, .. } if kind == "agent_message_chunk"),
        )
        .await;
    let prompt_id = log
        .seen()
        .iter()
        .rev()
        .find(|(d, m)| d == "send" && m["method"] == "session/prompt")
        .map(|(_, m)| m["id"].clone());
    drop(sent);
    tokio::time::sleep(Duration::from_secs(8)).await;
    let seen = log.seen();
    let cancel_sent = seen.iter().any(|(d, m)| {
        d == "send" && m["method"] == "$/cancel_request" && Some(&m["params"]["requestId"]) == prompt_id.as_ref()
    });
    let response = seen
        .iter()
        .find(|(d, m)| d == "recv" && m.get("method").is_none() && Some(&m["id"]) == prompt_id.as_ref())
        .map(|(_, m)| m.clone());
    out["dropPrompt"] = json!({
        "droppedAfterFirstChunk": first.is_some(),
        "sdkSentCancelRequest": cancel_sent,
        "adapterResponse": response.as_ref().map(|m| m.get("result").cloned().unwrap_or_else(|| m["error"].clone())),
    });

    // 8. close the session
    let close = cx
        .send_request(CloseSessionRequest::new(sid.clone()))
        .block_task()
        .await;
    out["sessionClose"] = json!(close.is_ok());
    Ok(out)
}
