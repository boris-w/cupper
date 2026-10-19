//! IT-handshake and IT-pinned-token: the gateway on a real socket, in this
//! test process.

use std::io::Write;
use std::os::fd::AsFd;
use std::process::{Command, Stdio};

use cupper_protocol::frame::{self, FrameType};
use cupper_protocol::ids::HostId;
use cupper_protocol::limits::V1;
use cupper_sys::{RefusalKind, Requirement};
use serde_json::{Value, json};

use super::testkit::{Client, TestGateway, initialize_params};
use super::{AuditEvent, CloseReason, Verdict};

/// `data.code` and `data.reason` of an error, `""` when missing.
fn code(error: &Value) -> (&str, &str) {
    let field = |name| error["data"][name].as_str().unwrap_or("");
    (field("code"), field("reason"))
}

fn peer() -> Value {
    json!({"type": "peer"})
}

#[tokio::test]
async fn it_handshake_hello_then_one_initialize_as_read_only_cli() {
    let mut gw = TestGateway::start(None);
    let mut client = Client::connect(&gw.socket).await;
    let hello = client.next().await.unwrap();
    assert_eq!(hello["method"], "host/hello");
    assert_eq!(hello["params"]["protocol"], json!({"min": 1, "max": 1}));
    assert_eq!(hello["params"]["host_version"], env!("CARGO_PKG_VERSION"));
    let nonce = hello["params"]["nonce"].as_str().unwrap();
    assert!(nonce.len() == 64 && nonce.bytes().all(|b| b.is_ascii_hexdigit()));

    let mut params = initialize_params(nonce, peer());
    params["protocol"] = json!({"min": 1, "max": 5});
    params["features"] = json!(["stream.coalesce", "canvas.v2"]);
    params["coalesce_ms"] = json!(5000);
    let result = client.call("initialize", params).await.unwrap();
    let host = json!({"host_id": HostId::from_uuid(1), "version": env!("CARGO_PKG_VERSION"),
        "weak_auth": cfg!(feature = "dev-auth"), "data_epoch": 1});
    let expected = json!({"protocol": 1, "host": host, "features": [], "limits": V1,
        "principal": {"id": "pr_cli", "kind": "cli", "scopes": ["read"]}});
    assert_eq!(result, expected);

    // `ping` with `params` left out or empty; nothing serves the rest yet.
    assert_eq!(client.call("ping", Value::Null).await, Ok(json!({})));
    assert_eq!(client.call("ping", json!({})).await, Ok(json!({})));
    let err = client.call("host/status", json!({})).await.unwrap_err();
    assert_eq!(code(&err), ("unsupported", ""));

    let again = initialize_params(nonce, peer());
    let err = client.call("initialize", again).await.unwrap_err();
    assert_eq!(code(&err), ("forbidden", "already_initialized"));
    assert_eq!(client.next().await, None, "closed after the refusal");

    let accepted = gw.record().await;
    assert_eq!(accepted.conn.as_str(), "c_1");
    let AuditEvent::Accepted { pid, verdict, .. } = accepted.event else {
        panic!("{accepted:?}");
    };
    assert_eq!(
        (pid, verdict),
        (std::process::id() as i32, Verdict::Other(None))
    );
    let initialized = gw.record().await.event;
    let AuditEvent::Initialized {
        principal,
        client: who,
        weak_auth,
    } = initialized
    else {
        panic!("{initialized:?}");
    };
    assert_eq!(principal.id.as_str(), "pr_cli");
    assert_eq!(
        (who.name.as_str(), weak_auth),
        ("testkit", cfg!(feature = "dev-auth"))
    );
    let closed = gw.record().await.event;
    let AuditEvent::Closed {
        reason, principal, ..
    } = closed
    else {
        panic!("{closed:?}");
    };
    assert_eq!(reason, CloseReason::AlreadyInitialized);
    assert_eq!(principal.unwrap().as_str(), "pr_cli");

    // Every connection gets a nonce of its own.
    assert_ne!(Client::connect(&gw.socket).await.hello().await, nonce);
}

#[tokio::test]
async fn it_handshake_failed_initialize_is_answered_then_closed() {
    // A requirement this test binary does not satisfy: the verdict is not
    // the UI, and the audit says why.
    let requirement = Requirement::new(r#"anchor apple generic and identifier "dev.cupper.app""#);
    let mut gw = TestGateway::start(Some(requirement));
    let codesign = match cfg!(feature = "dev-auth") {
        true => "bad_key",
        false => "signature_invalid",
    };
    let zeros = "0".repeat(64);
    let cases = [
        (
            "nonce",
            json!("ab".repeat(32)),
            ("unauthenticated", "bad_nonce"),
        ),
        (
            "protocol",
            json!({"min": 2, "max": 3}),
            ("version_unsupported", ""),
        ),
        (
            "auth",
            json!({"type": "codesign"}),
            ("unauthenticated", codesign),
        ),
        (
            "auth",
            json!({"type": "dev_key", "c_nonce": zeros, "proof": zeros}),
            ("unauthenticated", "bad_key"),
        ),
        (
            "auth",
            json!({"type": "harness", "c_nonce": zeros, "proof": zeros}),
            ("unauthenticated", "bad_key"),
        ),
        (
            "auth",
            json!({"type": "passkey"}),
            ("unauthenticated", "bad_key"),
        ),
        ("role", json!("control"), ("forbidden", "role")),
    ];
    for (field, value, expected) in cases {
        let mut client = Client::connect(&gw.socket).await;
        let mut params = initialize_params(&client.hello().await, peer());
        params[field] = value;
        let err = client.call("initialize", params).await.unwrap_err();
        assert_eq!(code(&err), expected, "{err}");
        if expected.0 == "version_unsupported" {
            assert_eq!(err["data"]["supported"], json!({"min": 1, "max": 1}));
        }
        assert_eq!(client.next().await, None, "closed after {err}");

        let accepted = gw.record().await.event;
        let AuditEvent::Accepted {
            verdict: Verdict::Other(Some(refusal)),
            ..
        } = accepted
        else {
            panic!("{accepted:?}");
        };
        assert_eq!(refusal.kind, RefusalKind::Unsatisfied, "{refusal}");
        let reason = gw.closed().await;
        assert!(
            matches!(&reason, CloseReason::HandshakeFailed(e)
            if e.app_code().map(ToString::to_string).as_deref() == Some(expected.0)),
            "{reason:?}"
        );
    }
}

#[tokio::test]
async fn it_handshake_first_message_must_be_initialize() {
    let mut gw = TestGateway::start(None);
    let not_json = frame::encode(FrameType::Json, b"{not json").unwrap();
    let blob = frame::encode(FrameType::BlobChunk, &[0; 41]).unwrap();
    let ping = frame::encode_json(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})).unwrap();
    let presence = json!({"jsonrpc": "2.0", "method": "client/presence",
        "params": {"foreground": true}});
    let initialize = json!({"jsonrpc": "2.0", "id": 2, "method": "initialize",
        "params": {"nonce": "x"}});
    type Expect = fn(&CloseReason) -> bool;
    let not_initialized: Expect = |reason| *reason == CloseReason::NotInitialized;
    let bad_params: Expect =
        |reason| matches!(reason, CloseReason::HandshakeFailed(err) if err.code == -32602);
    let cases = [
        (ping, Some(-32000), not_initialized),
        (not_json, Some(-32700), not_initialized),
        (
            frame::encode_json(&initialize).unwrap(),
            Some(-32602),
            bad_params,
        ),
        (
            frame::encode_json(&presence).unwrap(),
            None,
            not_initialized,
        ),
        (blob, None, not_initialized),
    ];
    for (frame, error, expect) in cases {
        let mut client = Client::connect(&gw.socket).await;
        client.hello().await;
        client.send_frame(&frame).await;
        if let Some(error) = error {
            let reply = client.next().await.unwrap();
            assert_eq!(reply["error"]["code"], error, "{reply}");
        }
        assert_eq!(client.next().await, None);
        let closed = gw.closed().await;
        assert!(expect(&closed), "{closed:?}");
    }
}

#[tokio::test]
async fn it_pinned_token_another_process_writing_on_the_connection_ends_it() {
    let mut gw = TestGateway::start(None);
    for initialized in [false, true] {
        let mut client = Client::connect(&gw.socket).await;
        let nonce = client.hello().await;
        let initialize = initialize_params(&nonce, peer());
        let message = if initialized {
            client.call("initialize", initialize).await.unwrap();
            json!({"jsonrpc": "2.0", "id": 9, "method": "ping"})
        } else {
            json!({"jsonrpc": "2.0", "id": 9, "method": "initialize", "params": initialize})
        };
        // `cat` writes the frame on the client's end: to the host, another
        // process is now talking on the verified connection. It stays alive
        // until the host has hung up, so the token names it.
        let end = client.stream().as_fd().try_clone_to_owned().unwrap();
        let mut cat = Command::new("/bin/cat")
            .stdin(Stdio::piped())
            .stdout(end)
            .spawn()
            .unwrap();
        let mut input = cat.stdin.take().unwrap();
        input
            .write_all(&frame::encode_json(&message).unwrap())
            .unwrap();
        assert_eq!(client.next().await, None, "no answer to the other process");
        drop(input);
        assert!(cat.wait().unwrap().success());
        assert_eq!(gw.closed().await, CloseReason::TokenMismatch);
    }
}

#[tokio::test]
async fn it_pinned_token_passed_fds_end_the_connection() {
    let mut gw = TestGateway::start(None);
    let mut client = Client::connect(&gw.socket).await;
    client.handshake(peer()).await.unwrap();
    let ping = frame::encode_json(&json!({"jsonrpc": "2.0", "id": 1, "method": "ping"})).unwrap();
    let (_read, write) = std::io::pipe().unwrap();
    let sent = cupper_sys::send_with_fds(client.stream(), &ping, &[write.as_fd()]).unwrap();
    assert_eq!(sent, ping.len());
    assert_eq!(client.next().await, None, "no answer to a message with fds");
    assert_eq!(gw.closed().await, CloseReason::FdPassed { fds: 1 });
}
