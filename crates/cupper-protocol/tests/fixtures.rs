//! UNIT-proto-fixtures: the samples in `protocol/fixtures/` that Rust and TS
//! share (design 5.3.10, ADR-04); `packages/protocol` checks the same files
//! with the generated zod schemas (UNIT-ts-fixtures).
//!
//! - `events.json`: `{name, expect, event, normalized?}`, where `expect` is
//!   `known`; `unknown`, a type this version does not know; `degraded`, a
//!   known type with malformed data, which strict decoding rejects and
//!   tolerant decoding turns into an unknown event as a whole (design
//!   5.3.11); or `invalid`, without an envelope or tag to degrade to. Decoded
//!   and encoded again, an event gives `normalized`, or itself when absent.
//! - `methods.json`: `{method, params, result?}`, typical messages of every
//!   method and notification in the catalog, each a round trip through its types.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use cupper_protocol::envelope::Event;
use cupper_protocol::event::{EventBody, Strictness};
use cupper_protocol::methods::catalog::catalog;
use cupper_protocol::methods::{admin, git, handshake, interaction, project, session, stream};
use cupper_protocol::rpc::{Method, Notification};
use cupper_protocol::schema::generator;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../protocol/fixtures")
}

fn cases(file: &str) -> Vec<Value> {
    let text = fs::read_to_string(dir().join(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{file}: {e}"))
}

#[test]
fn unit_proto_fixtures_every_file_is_checked() {
    let mut files: Vec<String> = fs::read_dir(dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    files.sort();
    assert_eq!(files, ["events.json", "methods.json"]);
}

#[test]
fn unit_proto_fixtures_events_decode_strictly_or_degrade() {
    let cases = cases("events.json");
    for case in &cases {
        let (name, event) = (&case["name"], &case["event"]);
        let strict = Event::decode(event, Strictness::Strict);
        let tolerant = Event::decode(event, Strictness::Tolerant);
        if case["expect"] == "invalid" {
            assert!(strict.is_err() && tolerant.is_err(), "{name}");
            continue;
        }
        let tolerant = tolerant.unwrap_or_else(|e| panic!("{name}: {e}"));
        let unknown = matches!(tolerant.value.body, EventBody::Unknown(_));
        let degraded = tolerant.degraded.is_some();
        match case["expect"].as_str() {
            Some("known") => assert!(strict.is_ok() && !unknown, "{name}"),
            Some("unknown") => assert!(strict.is_ok() && unknown && !degraded, "{name}"),
            Some("degraded") => assert!(strict.is_err() && unknown && degraded, "{name}"),
            other => panic!("{name}: no expectation {other:?}"),
        }
        let expected = case.get("normalized").unwrap_or(event);
        assert_eq!(
            &serde_json::to_value(&tolerant.value).unwrap(),
            expected,
            "{name}"
        );

        // The body alone, as snapshots and history pages carry it, decodes the same.
        let mut body = event.clone();
        body.as_object_mut()
            .unwrap()
            .retain(|key, _| key == "type" || key == "data");
        let bare = EventBody::decode(&body, Strictness::Tolerant).unwrap();
        assert_eq!(bare.value, tolerant.value.body, "{name}");
    }
    let expected: BTreeSet<&str> = ["known", "unknown", "degraded", "invalid"].into();
    let seen: BTreeSet<&str> = cases.iter().filter_map(|c| c["expect"].as_str()).collect();
    assert_eq!(seen, expected);
    let known = cases.iter().filter(|c| c["expect"] == "known");
    let types: BTreeSet<&str> = known.filter_map(|c| c["event"]["type"].as_str()).collect();
    assert_eq!(
        types,
        EventBody::TAGS.iter().copied().collect(),
        "every event type"
    );
}

/// `value` parses as `T` and serializes back to itself.
fn roundtrip<T: Serialize + DeserializeOwned>(method: &str, part: &str, value: &Value) {
    let parsed: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|e| panic!("{method} {part}: {e}: {value}"));
    let back = serde_json::to_value(parsed).unwrap();
    assert_eq!(&back, value, "{method} {part}");
}

/// Checks a `methods.json` case with the types of the method it names.
macro_rules! check_method {
    ($case:expr; requests: [$($request:ty),+ $(,)?]; notifications: [$($notification:ty),+ $(,)?]) => {{
        let case: &Value = $case;
        let name = case["method"].as_str().unwrap();
        $(if name == <$request as Method>::NAME {
            roundtrip::<<$request as Method>::Params>(name, "params", &case["params"]);
            roundtrip::<<$request as Method>::Result>(name, "result", &case["result"]);
        } else)+ $(if name == <$notification as Notification>::NAME {
            roundtrip::<<$notification as Notification>::Params>(name, "params", &case["params"]);
            assert!(case.get("result").is_none(), "{name}: a notification has no result");
        } else)+ {
            panic!("`{name}` is not in the list above");
        }
    }};
}

#[test]
fn unit_proto_fixtures_every_method_roundtrips() {
    let cases = cases("methods.json");
    for case in &cases {
        check_method!(case;
            requests: [
                handshake::Initialize, handshake::Ping, handshake::HostStatus,
                handshake::HostPrepareUpgrade, handshake::HostExit,
                stream::StreamSubscribe, stream::StreamUnsubscribe, stream::HistoryPage,
                stream::BlobGet, stream::BlobPut,
                git::GitStatus, git::GitDiff, git::GitFileDiff, git::GitStage, git::GitUnstage,
                git::GitRevert, git::GitCommit, git::GitPush, git::GitBranches,
                git::WorktreeCheck, git::FilesSearch, git::WorkspaceOpen,
                admin::SettingsGet, admin::SettingsUpdate, admin::AuthElevate,
                admin::AuthElevationGet, admin::AuthElevationRespond, admin::PolicyDescribe,
                admin::DiagExport, admin::DiagRecordingExport, admin::TestClockAdvance,
                admin::TestRunMaintenance, admin::TestSeed,
                project::ProjectAdd, project::ProjectTrust, project::ProjectTrustInfo,
                project::ProjectRemove, project::AgentsCapabilities, project::AgentsRefresh,
                project::AgentsRestart,
                session::SessionCreate, session::SessionSend, session::SessionInterrupt,
                session::SessionQueueResume, session::SessionQueueEdit, session::SessionSetConfig,
                session::SessionConfirmConfig, session::SessionRename, session::SessionArchive,
                session::SessionRestore, session::SessionMarkRead, session::SessionGet,
                session::SessionCapabilities, session::SessionListArchived,
                interaction::InteractionRespond, interaction::InteractionGet,
            ];
            notifications: [
                handshake::HostHello, handshake::ClientPresence, handshake::HostShutdown,
                handshake::UiNavigate, stream::StreamEvents, stream::StreamSnapshot,
                stream::StreamReset, stream::StreamAck,
            ]
        );
    }
    let sampled: BTreeSet<&str> = cases.iter().filter_map(|c| c["method"].as_str()).collect();
    let catalog = catalog(&mut generator());
    let tables = [
        &catalog.requests,
        &catalog.client_notifications,
        &catalog.host_notifications,
    ];
    let all: BTreeSet<&str> = tables.iter().flat_map(|t| t.keys().copied()).collect();
    assert_eq!(sampled, all, "every method of the catalog has a sample");
}
