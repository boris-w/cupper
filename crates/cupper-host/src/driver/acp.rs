//! The ACP connection to one adapter instance (design 5.6.1, 5.6.10): the
//! official SDK's `Client` over the [`Tap`]'s lines.
//!
//! - **Callbacks only forward** (ADR-02). Everything the agent sends lands on
//!   the caller's mailbox as a [`FromAgent`], requests with their
//!   [`Responder`]. `$/cancel_request` is the SDK's: it marks the responder's
//!   `cancellation()` (K5).
//! - **Unhandled messages** (K27). The last handler of the chain answers an
//!   unknown request with method not found and drops an unknown notification.
//!   Without it, SDK 3.3.0 parks any message that carries a `sessionId` for a
//!   session handler that never comes, and the agent waits forever.
//! - **No request is dropped in flight** (K28). Dropping a `SentRequest` or
//!   its future makes the SDK send `$/cancel_request`, and claude-agent-acp
//!   cancels the turn. [`AcpConnection::request`] hands each request to the
//!   SDK together with its reply callback, so no such value exists on the host
//!   side: giving up on a reply is ignoring it, and K1's fallback closes the
//!   session explicitly.
//! - `initialize` declares form elicitation, `_meta.terminal_output` and the
//!   unstable `plan`. `fs` and `terminal` are never supported: the host would
//!   then read, write and run things outside any sandbox for the agent, and
//!   changing that needs a security review.

use std::pin::pin;

use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::schema::v1::{
    ClientCapabilities, ContentBlock, CreateElicitationRequest, CreateElicitationResponse,
    ElicitationCapabilities, ElicitationFormCapabilities, FileSystemCapabilities, Implementation,
    InitializeRequest, InitializeResponse, Meta, PlanCapabilities, RequestPermissionRequest,
    RequestPermissionResponse, SessionId, SessionNotification,
};
use agent_client_protocol::util::internal_error;
use agent_client_protocol::{
    Agent, Client, ConnectionTo, Dispatch, Error, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, Responder, UntypedMessage,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, oneshot};

use super::tap::Tap;
use super::transport::AcpTransport;

/// `clientInfo.name`. Codex puts it in the User-Agent it sends to OpenAI, so it
/// says nothing about the user (K26, ADR-08).
pub const CLIENT_NAME: &str = "cupper";

/// claude-agent-acp's steering request; `initialize` tells whether the agent
/// takes it (`_meta.steering.supported`, design 5.6.7).
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "_session/steering", response = SteeringResponse)]
#[serde(rename_all = "camelCase")]
pub struct SteeringRequest {
    pub session_id: SessionId,
    pub prompt: Vec<ContentBlock>,
    #[serde(rename = "_meta", default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcResponse)]
pub struct SteeringResponse {
    /// `injected`, `startedNewTurn` or `promptRequired` so far (T003).
    pub outcome: String,
    #[serde(default)]
    pub reason: Option<String>,
}

/// The login state the adapter pushes (K16). Only the kind and the plan name
/// are read; the account's e-mail and organisation stay in the line.
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcNotification)]
#[notification(method = "_auth/status_update")]
#[serde(rename_all = "camelCase")]
pub struct AuthStatusUpdate {
    pub auth_status: AuthStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthStatus {
    /// `account`, `gateway` or `none` so far (SP-1).
    pub kind: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub account: Option<AuthAccount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthAccount {
    #[serde(default)]
    pub plan: Option<String>,
}

/// What the agent sent, as the SDK parsed it.
#[derive(Debug)]
pub enum FromAgent {
    Update(SessionNotification),
    AuthStatus(AuthStatusUpdate),
    Permission(
        RequestPermissionRequest,
        Responder<RequestPermissionResponse>,
    ),
    Elicitation(
        CreateElicitationRequest,
        Responder<CreateElicitationResponse>,
    ),
    /// The connection is over and nothing follows; an error says what ended it.
    Closed(Result<(), Error>),
}

/// An open connection. Dropping it ends the connection, which closes the
/// adapter's stdin.
#[derive(Debug)]
pub struct AcpConnection {
    cx: ConnectionTo<Agent>,
    _close: oneshot::Sender<()>,
}

impl AcpConnection {
    /// Connects over `transport` through `tap` and runs `initialize`. Until
    /// [`FromAgent::Closed`], what the agent sends goes to `mailbox` in the
    /// order it arrived.
    pub async fn open<M>(
        transport: impl AcpTransport,
        tap: Tap,
        mailbox: mpsc::UnboundedSender<M>,
    ) -> Result<(Self, InitializeResponse), Error>
    where
        M: From<FromAgent> + Send + 'static,
    {
        let forward = {
            let mailbox = mailbox.clone();
            move |msg: FromAgent| {
                let _ = mailbox.send(M::from(msg));
            }
        };
        let (update, auth, permission, elicitation) =
            (forward.clone(), forward.clone(), forward.clone(), forward);
        let (ready_tx, ready_rx) = oneshot::channel();
        let (close_tx, close_rx) = oneshot::channel::<()>();
        let connection = Client
            .builder()
            .on_receive_notification(
                async move |n: SessionNotification, _cx| {
                    update(FromAgent::Update(n));
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_notification(
                async move |n: AuthStatusUpdate, _cx| {
                    auth(FromAgent::AuthStatus(n));
                    Ok(())
                },
                agent_client_protocol::on_receive_notification!(),
            )
            .on_receive_request(
                async move |r: RequestPermissionRequest, responder, _cx| {
                    permission(FromAgent::Permission(r, responder));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_request(
                async move |r: CreateElicitationRequest, responder, _cx| {
                    elicitation(FromAgent::Elicitation(r, responder));
                    Ok(())
                },
                agent_client_protocol::on_receive_request!(),
            )
            .on_receive_dispatch(
                async |dispatch: Dispatch<UntypedMessage, UntypedMessage>, _cx| unhandled(dispatch),
                agent_client_protocol::on_receive_dispatch!(),
            )
            .connect_with(tap.wire(transport), async move |cx: ConnectionTo<Agent>| {
                // The connection's own task, so awaiting here holds up nothing.
                let init = cx.send_request(initialize_request()).block_task().await;
                let initialized = init.is_ok();
                if ready_tx.send(init.map(|init| (cx.clone(), init))).is_ok() && initialized {
                    futures::future::select(close_rx, pin!(cx.incoming_closed())).await;
                }
                Ok(())
            });
        tokio::spawn(async move {
            let _ = mailbox.send(M::from(FromAgent::Closed(connection.await)));
        });
        let (cx, init) = ready_rx.await.map_err(|_| {
            internal_error("the connection ended before `initialize` was answered")
        })??;
        Ok((
            Self {
                cx,
                _close: close_tx,
            },
            init,
        ))
    }

    /// Sends `request`; `on_reply` gets its result. The SDK holds back the
    /// agent's next message until `on_reply` returns, so a reply passed on to
    /// the mailbox follows everything the agent sent before it. Fails, without
    /// sending, only when the connection is gone.
    pub fn request<R: JsonRpcRequest>(
        &self,
        request: R,
        on_reply: impl FnOnce(Result<R::Response, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.cx
            .send_request(request)
            .on_receiving_result(async move |result| {
                on_reply(result);
                Ok(())
            })
    }

    pub fn notify(&self, notification: impl JsonRpcNotification) -> Result<(), Error> {
        self.cx.send_notification(notification)
    }
}

/// K27: the end of the handler chain.
fn unhandled(dispatch: Dispatch<UntypedMessage, UntypedMessage>) -> Result<(), Error> {
    match dispatch {
        Dispatch::Request(request, responder) => {
            responder.respond_with_error(Error::method_not_found().data(request.method))
        }
        Dispatch::Notification(_) => Ok(()),
        Dispatch::Response(result, router) => router.route_with_result(result),
    }
}

/// The `initialize` every connection starts with (design 5.6.1).
fn initialize_request() -> InitializeRequest {
    let meta = Meta::from_iter([("terminal_output".to_owned(), true.into())]);
    let capabilities = ClientCapabilities::new()
        .fs(FileSystemCapabilities::new())
        .terminal(false)
        .elicitation(ElicitationCapabilities::new().form(ElicitationFormCapabilities::new()))
        .plan(PlanCapabilities::new())
        .meta(meta);
    InitializeRequest::new(ProtocolVersion::V1)
        .client_capabilities(capabilities)
        .client_info(Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use agent_client_protocol::schema::v1::{
        CancelNotification, CloseSessionRequest, PermissionOptionKind, PromptRequest,
        RequestPermissionOutcome, TextContent,
    };
    use futures::future::{Either, select};
    use serde_json::{Value, json};
    use tokio::io::{
        AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
    };

    use super::*;
    use crate::clock::{Clock, ManualClock, SystemClock};
    use crate::driver::tap::{RawSink, k19};
    use crate::driver::transport::MemoryTransport;

    /// A stand-in agent on the other end of a memory transport: raw JSON lines.
    struct TestAgent {
        lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
        write: WriteHalf<DuplexStream>,
    }

    impl TestAgent {
        async fn send(&mut self, msg: Value) {
            self.write
                .write_all(format!("{msg}\n").as_bytes())
                .await
                .unwrap();
        }

        /// The next line the host wrote, or `None` once it closed the agent's stdin.
        async fn recv(&mut self) -> Option<Value> {
            let line = soon(self.lines.next_line()).await.unwrap()?;
            Some(serde_json::from_str(&line).unwrap())
        }
    }

    /// Fails the test when `fut` takes longer than five seconds.
    async fn soon<T>(fut: impl Future<Output = T>) -> T {
        let clock = SystemClock::new();
        let timer = clock.timer_at(clock.now_mono() + Duration::from_secs(5));
        match select(pin!(fut), pin!(timer)).await {
            Either::Left((value, _)) => value,
            Either::Right(_) => panic!("timed out"),
        }
    }

    fn wire() -> (MemoryTransport, Tap, TestAgent) {
        let (transport, agent) = MemoryTransport::pair(1 << 16);
        let (read, write) = tokio::io::split(agent);
        let lines = BufReader::new(read).lines();
        let tap = Tap::new(
            "instance-1",
            Arc::new(ManualClock::new()),
            RawSink::bounded().0,
        );
        (transport, tap, TestAgent { lines, write })
    }

    /// An open connection to a test agent, and the `initialize` it answered.
    async fn connect() -> (
        AcpConnection,
        TestAgent,
        mpsc::UnboundedReceiver<FromAgent>,
        Value,
    ) {
        let (transport, tap, mut agent) = wire();
        let (mailbox, inbox) = mpsc::unbounded_channel();
        let answer = async {
            let init = agent.recv().await.unwrap();
            let id = init["id"].clone();
            agent
                .send(json!({"jsonrpc": "2.0", "id": id, "result": {"protocolVersion": 1}}))
                .await;
            init
        };
        let (opened, init) = tokio::join!(AcpConnection::open(transport, tap, mailbox), answer);
        (opened.unwrap().0, agent, inbox, init)
    }

    fn prompt(session: &str) -> PromptRequest {
        PromptRequest::new(
            session.to_owned(),
            vec![ContentBlock::Text(TextContent::new("hi"))],
        )
    }

    #[tokio::test]
    async fn acp_initialize_declares_only_what_the_host_supports() {
        let (_conn, _agent, _inbox, init) = connect().await;
        assert_eq!(init["method"], "initialize");
        assert_eq!(init["params"]["clientInfo"]["name"], "cupper");
        assert_eq!(
            init["params"]["clientCapabilities"],
            json!({"fs": {"readTextFile": false, "writeTextFile": false}, "terminal": false,
                   "plan": {}, "auth": {"terminal": false}, "elicitation": {"form": {}},
                   "_meta": {"terminal_output": true}})
        );
    }

    #[tokio::test]
    async fn unit_acp_unhandled_requests_get_method_not_found_and_notifications_are_dropped() {
        let (_conn, mut agent, mut inbox, _) = connect().await;
        let unknown = |id: &str, params| json!({"jsonrpc": "2.0", "id": id, "method": "_x/unknown", "params": params});
        agent
            .send(unknown("with-session", json!({"sessionId": "s-1"})))
            .await;
        agent.send(unknown("without-session", json!({}))).await;
        // Declared unsupported, so it is just as unknown.
        let read = json!({"sessionId": "s-1", "path": "/etc/hosts"});
        agent
            .send(json!({"jsonrpc": "2.0", "id": "fs", "method": "fs/read_text_file", "params": read}))
            .await;
        agent
            .send(json!({"jsonrpc": "2.0", "method": "_x/unknown", "params": {"sessionId": "s-1"}}))
            .await;
        agent.send(json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s-1",
            "update": {"sessionUpdate": "agent_message_chunk", "content": {"type": "text", "text": "after"}}}})).await;

        for id in ["with-session", "without-session", "fs"] {
            let response = agent.recv().await.unwrap();
            assert_eq!(
                (&response["id"], &response["error"]["code"]),
                (&json!(id), &json!(-32601))
            );
        }
        // The unknown notification never reached the mailbox; the update after it did.
        assert!(matches!(
            soon(inbox.recv()).await,
            Some(FromAgent::Update(_))
        ));
    }

    #[tokio::test]
    async fn unit_acp_unhandled_without_the_last_handler_the_sdk_never_answers() {
        // What K27 works around; when an SDK upgrade fixes it, this test fails.
        let (transport, tap, mut agent) = wire();
        tokio::spawn(
            Client
                .builder()
                .connect_with(tap.wire(transport), async |_cx| {
                    futures::future::pending::<Result<(), Error>>().await
                }),
        );
        for (id, params) in [
            ("with-session", json!({"sessionId": "s-1"})),
            ("without-session", json!({})),
        ] {
            agent
                .send(json!({"jsonrpc": "2.0", "id": id, "method": "_x/unknown", "params": params}))
                .await;
        }
        // Answers go out in the order requests are handled.
        assert_eq!(agent.recv().await.unwrap()["id"], "without-session");
    }

    #[tokio::test]
    async fn unit_acp_detach_giving_up_on_a_reply_sends_no_cancel_request() {
        let (conn, mut agent, _inbox, _) = connect().await;
        let (reply_tx, reply_rx) = oneshot::channel();
        conn.request(prompt("s-1"), move |reply| drop(reply_tx.send(reply)))
            .unwrap();
        let sent = agent.recv().await.unwrap();
        assert_eq!(sent["method"], "session/prompt");

        // The prompt never comes back: stop waiting, interrupt, then close the session (K1).
        drop(reply_rx);
        conn.notify(CancelNotification::new("s-1")).unwrap();
        let (closed_tx, closed_rx) = oneshot::channel();
        conn.request(CloseSessionRequest::new("s-1"), move |reply| {
            drop(closed_tx.send(reply))
        })
        .unwrap();
        assert_eq!(agent.recv().await.unwrap()["method"], "session/cancel");
        let next = agent.recv().await.unwrap();
        assert_eq!(next["method"], "session/close", "{next}");

        // A late answer to the prompt is harmless.
        agent
            .send(json!({"jsonrpc": "2.0", "id": sent["id"], "result": {"stopReason": "end_turn"}}))
            .await;
        agent
            .send(json!({"jsonrpc": "2.0", "id": next["id"], "result": {}}))
            .await;
        soon(closed_rx).await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn unit_acp_detach_dropping_an_sdk_request_sends_cancel_request() {
        // The trap K28 avoids: the SDK cancels a request whose future is dropped.
        let (conn, mut agent, _inbox, _) = connect().await;
        drop(conn.cx.send_request(prompt("s-1")).block_task());
        let sent = agent.recv().await.unwrap();
        let cancel = agent.recv().await.unwrap();
        assert_eq!(
            (&cancel["method"], &cancel["params"]["requestId"]),
            (&json!("$/cancel_request"), &sent["id"])
        );
    }

    #[tokio::test]
    async fn acp_extension_messages_and_k19_go_through_the_sdk() {
        let (conn, mut agent, mut inbox, _) = connect().await;

        agent.send(json!({"jsonrpc": "2.0", "method": "_auth/status_update", "params": {"authStatus":
            {"kind": "account", "label": "Claude Max", "account": {"plan": "max", "email": "someone@example.com"}}}})).await;
        let Some(FromAgent::AuthStatus(auth)) = soon(inbox.recv()).await else {
            panic!()
        };
        assert_eq!(
            auth.auth_status.account.and_then(|a| a.plan).as_deref(),
            Some("max")
        );

        let (reply_tx, reply_rx) = oneshot::channel();
        let steer = SteeringRequest {
            session_id: "s-1".into(),
            prompt: prompt("s-1").prompt,
            meta: None,
        };
        conn.request(steer, move |reply| drop(reply_tx.send(reply)))
            .unwrap();
        let sent = agent.recv().await.unwrap();
        assert_eq!(
            (&sent["method"], &sent["params"]["sessionId"]),
            (&json!("_session/steering"), &json!("s-1"))
        );
        agent
            .send(json!({"jsonrpc": "2.0", "id": sent["id"], "result": {"outcome": "injected"}}))
            .await;
        assert_eq!(soon(reply_rx).await.unwrap().unwrap().outcome, "injected");

        // A permission request with a kind the SDK does not know, then withdrawn.
        agent.send(json!({"jsonrpc": "2.0", "id": 7, "method": "session/request_permission", "params": {
            "sessionId": "s-1", "toolCall": {"toolCallId": "t-1"},
            "options": [{"optionId": "o", "name": "Always", "kind": "allow_forever"}]}})).await;
        let Some(FromAgent::Permission(ask, responder)) = soon(inbox.recv()).await else {
            panic!()
        };
        assert_eq!(ask.options[0].kind, PermissionOptionKind::RejectOnce);
        assert_eq!(
            k19::original(ask.options[0].meta.as_ref(), "kind"),
            Some(&json!("allow_forever"))
        );
        agent
            .send(
                json!({"jsonrpc": "2.0", "method": "$/cancel_request", "params": {"requestId": 7}}),
            )
            .await;
        soon(responder.cancellation().cancelled()).await;
        responder
            .respond(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ))
            .unwrap();
        assert_eq!(
            agent.recv().await.unwrap()["result"]["outcome"]["outcome"],
            "cancelled"
        );

        // Dropping the connection closes the agent's stdin and is reported last.
        drop(conn);
        assert_eq!(agent.recv().await, None);
        assert!(matches!(
            soon(inbox.recv()).await,
            Some(FromAgent::Closed(Ok(())))
        ));
    }
}
