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
//! - `views.json`: event bodies and the views they fold into, which
//!   `view::parity` writes and checks (UNIT-views-parity).
//!
//! UNIT-proto-absent-not-null walks the same samples, as the Rust types
//! encode them, along the exported schema: no optional field is `null`. So
//! does it instances of every event, method and error with every property
//! filled (`full`), then each with one optional property dropped (T193).

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use cupper_protocol::envelope::Event;
use cupper_protocol::event::{EventBody, Strictness};
use cupper_protocol::methods::catalog::catalog;
use cupper_protocol::methods::{admin, git, handshake, interaction, project, session, stream};
use cupper_protocol::rpc::{Method, Notification};
use cupper_protocol::schema::{document, generator};
use schemars::generate::SchemaSettings;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};

#[path = "fixtures/full.rs"]
mod full;

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
    assert_eq!(files, ["events.json", "methods.json", "views.json"]);
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
fn roundtrip<T: Serialize + DeserializeOwned>(_: &mut (), method: &str, part: &str, value: &Value) {
    let parsed: T = serde_json::from_value(value.clone())
        .unwrap_or_else(|e| panic!("{method} {part}: {e}: {value}"));
    let back = serde_json::to_value(parsed).unwrap();
    assert_eq!(&back, value, "{method} {part}");
}

/// Calls `$check::<T>(ctx, method, part, value)` on a `methods.json` case
/// with the types of the method it names.
macro_rules! check_method {
    ($check:ident($ctx:expr); $case:expr; requests: [$($request:ty),+ $(,)?]; notifications: [$($notification:ty),+ $(,)?]) => {{
        let case: &Value = $case;
        let name = case["method"].as_str().unwrap();
        $(if name == <$request as Method>::NAME {
            $check::<<$request as Method>::Params>($ctx, name, "params", &case["params"]);
            $check::<<$request as Method>::Result>($ctx, name, "result", &case["result"]);
        } else)+ $(if name == <$notification as Notification>::NAME {
            $check::<<$notification as Notification>::Params>($ctx, name, "params", &case["params"]);
            assert!(case.get("result").is_none(), "{name}: a notification has no result");
        } else)+ {
            panic!("`{name}` is not in the list above");
        }
    }};
}

/// [`check_method!`] with every method of the catalog.
macro_rules! each_method {
    ($check:ident($ctx:expr); $case:expr) => {
        check_method!($check($ctx); $case;
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
        )
    };
}

#[test]
fn unit_proto_fixtures_every_method_roundtrips() {
    let cases = cases("methods.json");
    for case in &cases {
        each_method!(roundtrip(&mut ()); case);
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

/// UNIT-proto-absent-not-null's walk (T190, design 5.3.1): a value along
/// the exported schema. A `null` on a property that is not required is a
/// fault, unless the property's own type takes `null` (a definition such as
/// `CliPath`). Optional properties seen, with a value or an allowed `null`,
/// are counted by `<definition>.<property>`, and so are those absent from
/// an object walked; `present` holds where the last ones seen were, as the
/// JSON pointer of their object and their key.
#[derive(Default)]
struct Walk {
    defs: Map<String, Value>,
    methods: Value,
    root: String,
    faults: Vec<String>,
    reached: BTreeSet<String>,
    absent: BTreeSet<String>,
    present: Vec<(String, String)>,
}

impl Walk {
    fn new() -> Self {
        let mut document = document();
        let Value::Object(defs) = document["$defs"].take() else {
            unreachable!("the document has $defs")
        };
        let methods = document["x-methods"].take();
        Walk {
            defs,
            methods,
            ..Walk::default()
        }
    }

    /// The methods of every table of the catalog, with their entries.
    fn entries(&self) -> impl Iterator<Item = (&String, &Value)> {
        let tables = self
            .methods
            .as_object()
            .into_iter()
            .flat_map(|t| t.values());
        tables.flat_map(|t| t.as_object().into_iter().flatten())
    }

    fn check(&mut self, root: &str, schema: &Value, value: &Value, owner: &str) {
        self.root = root.to_owned();
        self.walk(schema, value, owner, "");
    }

    /// The definition a `$ref` names, and its name.
    fn def<'a>(&'a self, schema: &Value) -> Option<(&'a str, &'a Value)> {
        let name = schema.get("$ref")?.as_str()?.trim_start_matches("#/$defs/");
        self.defs.get_key_value(name).map(|(k, v)| (k.as_str(), v))
    }

    fn nullable(&self, schema: &Value) -> bool {
        let null = |s: &Value| s.get("type").is_some_and(|t| t == "null");
        let types = schema.get("type").and_then(Value::as_array);
        self.def(schema).is_some_and(|(_, def)| self.nullable(def))
            || types.is_some_and(|t| t.iter().any(|t| t == "null"))
            || schema
                .get("anyOf")
                .and_then(Value::as_array)
                .is_some_and(|b| b.iter().any(null))
    }

    /// Whether `value` can be `schema`: its type, and the constants it tags.
    fn fits(&self, schema: &Value, value: &Value) -> bool {
        // A `$ref` with tags beside it, as a branch of a tagged union, fits both.
        if let Some((name, def)) = self.def(schema)
            && (name.starts_with("Unknown") || !self.fits(def, value))
        {
            return name.starts_with("Unknown");
        }
        let kind = match value {
            Value::Null => "null",
            Value::Bool(_) => "boolean",
            Value::Number(n) => {
                if n.is_f64() {
                    "number"
                } else {
                    "integer"
                }
            }
            Value::String(_) => "string",
            Value::Array(_) => "array",
            Value::Object(_) => "object",
        };
        let typed = schema
            .get("type")
            .is_none_or(|t| t == kind || t.as_array().is_some_and(|t| t.iter().any(|t| t == kind)));
        let props = schema.get("properties").and_then(Value::as_object);
        let tags = props
            .into_iter()
            .flatten()
            .filter_map(|(k, p)| Some((k, p.get("const")?)));
        typed && tags.into_iter().all(|(k, tag)| value.get(k) == Some(tag))
    }

    fn walk(&mut self, schema: &Value, value: &Value, owner: &str, at: &str) {
        if let Some((name, def)) = self.def(schema) {
            if !name.starts_with("Unknown") {
                let (name, def) = (name.to_owned(), def.clone());
                self.walk(&def, value, &name, at);
            }
            return;
        }
        for part in schema
            .get("allOf")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.walk(part, value, owner, at);
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(branches) = schema.get(key).and_then(Value::as_array)
                && let Some(branch) = branches.iter().find(|b| self.fits(b, value)).cloned()
            {
                self.walk(&branch, value, owner, at);
            }
        }
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let props = schema
            .get("properties")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        match value {
            Value::Object(members) => {
                let optional = props
                    .keys()
                    .filter(|k| !required.iter().any(|r| r == k.as_str()));
                let gone = optional.filter(|k| !members.contains_key(*k));
                self.absent.extend(gone.map(|k| format!("{owner}.{k}")));
                for (key, member) in members {
                    let step = key.replace('~', "~0").replace('/', "~1");
                    let (path, label) = (format!("{at}/{step}"), format!("{owner}.{key}"));
                    let Some(property) = props.get(key) else {
                        if let Some(each) =
                            schema.get("additionalProperties").filter(|s| s.is_object())
                        {
                            self.walk(&each.clone(), member, owner, &path);
                        }
                        continue;
                    };
                    if !required.iter().any(|r| r == key.as_str()) {
                        if member.is_null() && !self.nullable(property) {
                            let root = &self.root;
                            self.faults
                                .push(format!("{root} {path}: null on optional {label}"));
                        } else {
                            self.reached.insert(label.clone());
                            self.present.push((at.to_owned(), key.clone()));
                        }
                    }
                    self.walk(property, member, &label, &path);
                }
            }
            Value::Array(items) => {
                let Some(item) = schema.get("items") else {
                    return;
                };
                for (n, member) in items.iter().enumerate() {
                    self.walk(&item.clone(), member, owner, &format!("{at}/{n}"));
                }
            }
            _ => {}
        }
    }
}

type Decode<'a> = &'a dyn Fn(&Value) -> Result<Value, String>;

/// `decode` reads `value` as its Rust type and writes it again, and the walk
/// checks what it writes; then the same with each optional property it had
/// dropped in turn (T193): a missing `skip_serializing_if` turns that into `null`.
fn guard(walk: &mut Walk, root: &str, schema: &Value, value: &Value, owner: &str, decode: Decode) {
    let once = |walk: &mut Walk, value: &Value| match decode(value) {
        Ok(encoded) => walk.check(root, schema, &encoded, owner),
        Err(e) => walk
            .faults
            .push(format!("{root}: does not decode: {e}: {value}")),
    };
    walk.present.clear();
    once(walk, value);
    for (object, key) in std::mem::take(&mut walk.present) {
        let mut dropped = value.clone();
        if let Some(Value::Object(members)) = dropped.pointer_mut(&object)
            && members.remove(&key).is_some()
        {
            once(walk, &dropped);
        }
    }
}

fn decode<T: Serialize + DeserializeOwned>(value: &Value) -> Result<Value, String> {
    let decoded = serde_json::from_value::<T>(value.clone()).map_err(|e| e.to_string())?;
    Ok(serde_json::to_value(decoded).unwrap())
}

/// `value` as `T` decodes and encodes it, guarded along `method`'s `part`.
fn absent<T: Serialize + DeserializeOwned>(
    walk: &mut Walk,
    method: &str,
    part: &str,
    value: &Value,
) {
    let entry = walk
        .entries()
        .find(|(m, _)| *m == method)
        .unwrap()
        .1
        .clone();
    let root = format!("{method} {part}");
    guard(walk, &root, &entry[part], value, method, &decode::<T>);
}

/// The optional properties that were an `Option`: nullable before the
/// export dropped their `null`, by `<definition>.<property>`.
fn options() -> BTreeSet<String> {
    fn collect(schema: &Value, owner: &str, out: &mut BTreeSet<String>) {
        let required = schema
            .get("required")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for (key, property) in schema
            .get("properties")
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
        {
            let label = format!("{owner}.{key}");
            let null = |s: &Value| {
                s.get("type").is_some_and(|t| {
                    t == "null" || t.as_array().is_some_and(|t| t.iter().any(|t| t == "null"))
                })
            };
            let any = property.get("anyOf").and_then(Value::as_array);
            if !required.iter().any(|r| r == key.as_str())
                && (null(property) || any.is_some_and(|b| b.iter().any(null)))
            {
                out.insert(label.clone());
            }
            collect(property, &label, out);
        }
        for key in ["anyOf", "oneOf", "allOf", "items", "additionalProperties"] {
            let parts = schema
                .get(key)
                .map(|p| p.as_array().cloned().unwrap_or_else(|| vec![p.clone()]));
            parts
                .into_iter()
                .flatten()
                .for_each(|p| collect(&p, owner, out));
        }
    }
    let mut g = SchemaSettings::draft2020_12().into_generator();
    catalog(&mut g);
    g.subschema_for::<Event<EventBody>>();
    g.subschema_for::<cupper_protocol::view::IndexView>();
    g.subschema_for::<cupper_protocol::view::SessionView>();
    g.subschema_for::<cupper_protocol::error::RpcError>();
    let mut out = BTreeSet::new();
    for (name, def) in g.take_definitions(true) {
        collect(&def, &name, &mut out);
    }
    out
}

#[test]
fn unit_proto_absent_not_null_samples_send_no_null_on_optional_fields() {
    let mut walk = Walk::new();
    let event = json!({"$ref": "#/$defs/Event"});
    let read = |strictness| {
        move |value: &Value| {
            let decoded = Event::decode(value, strictness).map_err(|e| e.to_string())?;
            Ok(serde_json::to_value(decoded.value).unwrap())
        }
    };
    let (tolerant, strict) = (read(Strictness::Tolerant), read(Strictness::Strict));
    for case in cases("events.json")
        .iter()
        .filter(|c| c["expect"] != "invalid")
    {
        let root = format!("event {}", case["name"]);
        guard(&mut walk, &root, &event, &case["event"], "Event", &tolerant);
    }
    for case in &cases("methods.json") {
        each_method!(absent(&mut walk); case);
    }
    for case in &cases("views.json") {
        let view = if case["view"] == "index" {
            "IndexView"
        } else {
            "SessionView"
        };
        let (name, schema) = (&case["name"], json!({"$ref": format!("#/$defs/{view}")}));
        walk.check(&format!("view {name}"), &schema, &case["expected"], view);
        for op in case["ops"].as_array().unwrap() {
            let body = json!({"$ref": "#/$defs/EventBody"});
            walk.check(&format!("view {name} op"), &body, op, "EventBody");
        }
    }

    // Every event, method and error with every property filled (T193).
    for value in full::full(&walk.defs, &event) {
        let root = format!("full event {}", value["type"]);
        guard(&mut walk, &root, &event, &value, "Event", &strict);
    }
    let mut calls = Vec::new();
    for (method, entry) in walk.entries() {
        let params = full::full(&walk.defs, &entry["params"]);
        let results = entry.get("result").map(|r| full::full(&walk.defs, r));
        let n = params.len().max(results.as_ref().map_or(0, Vec::len));
        for i in 0..n {
            let mut call = json!({"method": method, "params": params[i % params.len()]});
            if let Some(results) = &results {
                call["result"] = results[i % results.len()].clone();
            }
            calls.push(call);
        }
    }
    for call in &calls {
        each_method!(absent(&mut walk); call);
    }
    let error = json!({"$ref": "#/$defs/RpcError"});
    for value in full::full(&walk.defs, &error) {
        let decode = decode::<cupper_protocol::error::RpcError>;
        guard(&mut walk, "full error", &error, &value, "RpcError", &decode);
    }
    assert_eq!(walk.faults, Vec::<String>::new());

    let options = options();
    let missed = |seen: &BTreeSet<String>| options.difference(seen).cloned().collect::<Vec<_>>();
    let (unreached, unchecked) = (missed(&walk.reached), missed(&walk.absent));
    eprintln!(
        "{} of {} Option properties reached, {} checked absent; not checked absent:",
        options.len() - unreached.len(),
        options.len(),
        options.len() - unchecked.len()
    );
    unchecked.iter().for_each(|o| eprintln!("  {o}"));
    assert!(unreached.is_empty(), "not reached: {unreached:?}");
}
