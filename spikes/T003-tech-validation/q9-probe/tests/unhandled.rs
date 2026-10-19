//! What SDK =3.3.0 does with an agent request that no handler claims.
//!
//! The Client role's default handler marks any unclaimed message that carries
//! `params.sessionId` as "retry later" (it waits for a per-session dynamic
//! handler), so such a request is never answered and such a notification is
//! kept in memory. Unclaimed messages without `sessionId` are answered with
//! "method not found" or dropped. A last, untyped `on_receive_dispatch`
//! handler closes the gap. A fake agent on an in-memory pipe shows both cases;
//! no real agent or model is involved.

use std::time::Duration;

use agent_client_protocol::{Agent, ByteStreams, Client, ConnectionTo, Dispatch, UntypedMessage};
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

/// Sends two unknown requests from the fake agent (with and without
/// `sessionId`) and returns the responses that came back within one second.
async fn responses(catch_all: bool) -> Vec<Value> {
    let (client_side, agent_side) = tokio::io::duplex(1 << 16);
    let (client_read, client_write) = tokio::io::split(client_side);
    let transport = ByteStreams::new(client_write.compat_write(), client_read.compat());

    let client = tokio::spawn(async move {
        let run = async |_cx: ConnectionTo<Agent>| {
            tokio::time::sleep(Duration::from_secs(2)).await;
            Ok(())
        };
        if catch_all {
            Client
                .builder()
                .on_receive_dispatch(
                    async |d: Dispatch<UntypedMessage, UntypedMessage>, _cx| match d {
                        Dispatch::Request(_, responder) => {
                            responder.respond_with_error(agent_client_protocol::Error::method_not_found())
                        }
                        Dispatch::Notification(_) => Ok(()),
                        Dispatch::Response(result, router) => router.route_with_result(result),
                    },
                    agent_client_protocol::on_receive_dispatch!(),
                )
                .connect_with(transport, run)
                .await
        } else {
            Client.builder().connect_with(transport, run).await
        }
    });

    let (agent_read, mut agent_write) = tokio::io::split(agent_side);
    for (id, params) in [
        ("with-session", json!({ "sessionId": "s1" })),
        ("without-session", json!({})),
    ] {
        let line = json!({ "jsonrpc": "2.0", "id": id, "method": "_example/unknown", "params": params });
        agent_write.write_all(format!("{line}\n").as_bytes()).await.unwrap();
    }
    let mut lines = BufReader::new(agent_read).lines();
    let mut out = Vec::new();
    while let Ok(Ok(Some(line))) = tokio::time::timeout(Duration::from_secs(1), lines.next_line()).await {
        out.push(serde_json::from_str(&line).unwrap());
    }
    drop(agent_write);
    let _ = client.await;
    out
}

fn answered(responses: &[Value], id: &str) -> bool {
    responses.iter().any(|r| r["id"] == id && r.get("error").is_some())
}

#[tokio::test]
async fn unknown_request_with_session_id_is_never_answered_by_default() {
    let r = responses(false).await;
    assert!(answered(&r, "without-session"), "{r:?}");
    assert!(!answered(&r, "with-session"), "{r:?}");
}

#[tokio::test]
async fn a_final_untyped_handler_answers_it() {
    let r = responses(true).await;
    assert!(answered(&r, "without-session"), "{r:?}");
    assert!(answered(&r, "with-session"), "{r:?}");
}
