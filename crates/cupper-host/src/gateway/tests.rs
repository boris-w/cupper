//! IT-handshake and IT-pinned-token: the gateway on a real socket, in this
//! test process, which also counts as the process that started the host.

use std::io::Write;
use std::os::fd::AsFd;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use cupper_protocol::frame::{self, FrameType};
use cupper_protocol::ids::HostId;
use cupper_protocol::limits::V1;
use cupper_protocol::proof::Direction;
use cupper_sys::{RefusalKind, Requirement};
use serde_json::{Value, json};

use super::testkit::{Client, KEY, TestGateway, initialize_params, key_auth};
use super::{AuditEvent, CloseReason, Routes, Verdict};
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
use crate::clock::SystemClock;
use crate::clock::{ManualClock, MonoTime};

/// `data.code` and `data.reason` of an error, `""` when missing.
fn code(error: &Value) -> (&str, &str) {
    let field = |name| error["data"][name].as_str().unwrap_or("");
    (field("code"), field("reason"))
}

fn peer() -> Value {
    json!({"type": "peer"})
}

fn me() -> i32 {
    std::process::id() as i32
}

/// `initialize` with `auth`; `dev_key` is the desktop UI's, in a role.
fn key_params(nonce: &str, auth: Value) -> Value {
    let mut params = initialize_params(nonce, auth);
    if params["auth"]["type"] == "dev_key" {
        params["client"]["kind"] = json!("desktop");
        params["role"] = json!("control");
    }
    params
}

/// The error of an `initialize` whose `auth` `make` builds from the nonce;
/// the connection closes after it.
#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
async fn refused(gw: &mut TestGateway, make: impl Fn(&str) -> Value) -> Value {
    let mut client = Client::connect(&gw.socket).await;
    let nonce = client.hello().await;
    let params = key_params(&nonce, make(&nonce));
    let err = client.call("initialize", params).await.unwrap_err();
    assert_eq!(client.next().await, None, "closed after {err}");
    assert!(matches!(gw.closed().await, CloseReason::HandshakeFailed(_)));
    err
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
    let expected = json!({"protocol": 1, "host": host, "features": ["stream.coalesce"], "limits": V1,
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
    let key_refusal = |offered| match offered {
        true => "bad_proof",
        false => "bad_key",
    };
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
            ("unauthenticated", key_refusal(cfg!(feature = "dev-auth"))),
        ),
        (
            "auth",
            json!({"type": "harness", "c_nonce": zeros, "proof": zeros}),
            (
                "unauthenticated",
                key_refusal(cfg!(feature = "test-harness")),
            ),
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
async fn it_handshake_key_auths_where_offered_prove_the_host_too() {
    let gw = TestGateway::start(None);
    let ui = json!({"id": "pr_desktop", "kind": "desktop_ui", "role": "control",
        "scopes": ["read", "write", "approve", "privileged", "ui_local"]});
    let test = json!({"id": "pr_test", "kind": "test_harness",
        "scopes": ["read", "write", "approve", "privileged", "ui_local", "test"]});
    let kinds = [
        ("dev_key", cfg!(feature = "dev-auth"), ui),
        ("harness", cfg!(feature = "test-harness"), test),
    ];
    for (kind, offered, principal) in kinds {
        let mut client = Client::connect(&gw.socket).await;
        let nonce = client.hello().await;
        let (auth, channel) = key_auth(kind, &KEY, &nonce, me());
        let result = client.call("initialize", key_params(&nonce, auth)).await;
        if !offered {
            assert_eq!(code(&result.unwrap_err()), ("unauthenticated", "bad_key"));
            continue;
        }
        let result = result.unwrap();
        assert_eq!(result["principal"], principal, "{kind}");
        // The host's proof, bound to this connection and to its own pid.
        let host_proof = result["host_proof"].as_str().unwrap();
        let checked = channel.check(&KEY, Direction::HostToClient, me(), host_proof);
        assert_eq!(checked, Ok(()), "{kind}");
        assert_eq!(client.call("ping", Value::Null).await, Ok(json!({})));
    }
}

#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
#[tokio::test]
async fn it_handshake_wrong_or_missing_key_proofs_are_refused() {
    let mut gw = TestGateway::start(None);
    let kinds = [
        #[cfg(feature = "dev-auth")]
        "dev_key",
        #[cfg(feature = "test-harness")]
        "harness",
    ];
    for kind in kinds {
        for case in [
            "relayed",
            "other key",
            "other nonce",
            "no proof",
            "bad c_nonce",
        ] {
            let make = |nonce: &str| {
                let good = key_auth(kind, &KEY, nonce, me()).0;
                match case {
                    // A process relaying the handshake proves with its own pid.
                    "relayed" => key_auth(kind, &KEY, nonce, me() + 1).0,
                    "other key" => key_auth(kind, &[1; 32], nonce, me()).0,
                    "other nonce" => key_auth(kind, &KEY, &"ab".repeat(32), me()).0,
                    "no proof" => json!({"type": kind, "c_nonce": good["c_nonce"]}),
                    _ => json!({"type": kind, "c_nonce": "c0", "proof": good["proof"]}),
                }
            };
            let err = refused(&mut gw, make).await;
            assert_eq!(
                code(&err),
                ("unauthenticated", "bad_proof"),
                "{kind} {case}"
            );
        }
    }
}

#[cfg(any(feature = "dev-auth", feature = "test-harness"))]
#[tokio::test]
async fn it_handshake_key_auths_need_the_key_and_dev_key_its_launcher() {
    let mut config = TestGateway::config(None);
    let mut cases = Vec::new();
    #[cfg(feature = "dev-auth")]
    {
        // Started by another process than this one, and without a key.
        let mut other = TestGateway::config(None);
        other.dev_key.as_mut().unwrap().launcher ^= 1;
        cases.push((other, "dev_key", "started the host"));
        config.dev_key = None;
        cases.push((config.clone(), "dev_key", "no key"));
    }
    #[cfg(feature = "test-harness")]
    {
        config.harness_key = None;
        cases.push((config, "harness", "no key"));
    }
    for (config, kind, message) in cases {
        let mut gw = TestGateway::with(config, Arc::new(SystemClock::new()), Routes::default());
        let err = refused(&mut gw, |nonce| key_auth(kind, &KEY, nonce, me()).0).await;
        assert_eq!(code(&err), ("unauthenticated", "bad_key"), "{kind}");
        assert!(err["message"].as_str().unwrap().contains(message), "{err}");
    }
}

#[tokio::test]
async fn it_handshake_initialize_within_ten_seconds_of_hello() {
    let clock = Arc::new(ManualClock::new());
    let mut gw = TestGateway::with(TestGateway::config(None), clock.clone(), Routes::default());
    let mut prompt = Client::connect(&gw.socket).await;
    prompt.handshake(peer()).await.unwrap();
    clock.advance(Duration::from_secs(5));
    let mut late = Client::connect(&gw.socket).await;
    late.hello().await;
    // Only the connection without `initialize` has a deadline.
    let deadline = MonoTime::from_origin(Duration::from_secs(15));
    assert_eq!(clock.next_deadline(), Some(deadline));
    clock.advance(Duration::from_millis(9_999));
    assert_eq!(clock.next_deadline(), Some(deadline), "not yet");
    clock.advance(Duration::from_millis(1));
    assert_eq!(late.next().await, None);
    assert_eq!(gw.closed().await, CloseReason::HandshakeTimeout);
    assert_eq!(prompt.call("ping", Value::Null).await, Ok(json!({})));
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

/// IT-lifecycle at the gateway: on the way out, `host/shutdown` reaches the
/// initialized connections, then every connection ends.
#[tokio::test]
async fn it_lifecycle_gateway_shutdown_reaches_clients_then_all_connections_end() {
    use cupper_protocol::domain::PrincipalKind;
    use cupper_protocol::index::ClientAuth;
    use cupper_protocol::methods::handshake::{HostShutdown, HostShutdownParams, ShutdownReason};

    let mut gw = TestGateway::start(None);
    let mut ready = Client::connect(&gw.socket).await;
    ready.handshake(peer()).await.expect("initialized");
    let mut waiting = Client::connect(&gw.socket).await;
    waiting.hello().await;

    let clients = gw.gateway.clients();
    assert_eq!(clients.len(), 1, "{clients:?}");
    assert_eq!(clients[0].principal_kind, PrincipalKind::Cli);
    assert_eq!(clients[0].auth, ClientAuth::Peer);
    assert_eq!(clients[0].client_name, "testkit");

    gw.gateway.broadcast::<HostShutdown>(&HostShutdownParams {
        reason: ShutdownReason::UserQuit,
        restart_expected: false,
    });
    gw.gateway.close_all().await;
    let shutdown = ready.next().await.expect("host/shutdown");
    assert_eq!(shutdown["method"], "host/shutdown", "{shutdown}");
    assert_eq!(shutdown["params"]["reason"], "user_quit");
    assert_eq!(ready.next().await, None);
    assert_eq!(waiting.next().await, None);
    for _ in 0..2 {
        assert_eq!(gw.closed().await, CloseReason::HostExiting);
    }
    assert!(gw.gateway.clients().is_empty());
}
