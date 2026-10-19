//! Every method and notification by name, as schema entries (design 5.3.4,
//! 5.3.10): what the schema export writes, and what the tests below check
//! method by method. A new group adds one `register` line to [`catalog`].

use std::collections::BTreeMap;

use schemars::SchemaGenerator;
use serde::Serialize;
use serde_json::Value;

use super::{handshake, project, session, stream};
use crate::rpc::{ClientNotification, HostNotification, Method};
use crate::schema::{client_notification_entry, host_notification_entry, method_entry};

/// The three tables of entries, each sorted by name, so the export does not
/// depend on the order of registration or of `serde_json` maps.
#[derive(Debug, Default, Serialize)]
pub struct Catalog {
    pub requests: BTreeMap<&'static str, Value>,
    pub client_notifications: BTreeMap<&'static str, Value>,
    pub host_notifications: BTreeMap<&'static str, Value>,
}

impl Catalog {
    pub fn request<M: Method>(&mut self, g: &mut SchemaGenerator) {
        insert(&mut self.requests, M::NAME, method_entry::<M>(g));
    }

    pub fn client_notification<N: ClientNotification>(&mut self, g: &mut SchemaGenerator) {
        let entry = client_notification_entry::<N>(g);
        insert(&mut self.client_notifications, N::NAME, entry);
    }

    pub fn host_notification<N: HostNotification>(&mut self, g: &mut SchemaGenerator) {
        let entry = host_notification_entry::<N>(g);
        insert(&mut self.host_notifications, N::NAME, entry);
    }
}

fn insert(table: &mut BTreeMap<&'static str, Value>, name: &'static str, entry: Value) {
    assert!(
        table.insert(name, entry).is_none(),
        "{name} registered twice"
    );
}

/// Every method and notification. Their types land in `g`'s definitions.
pub fn catalog(g: &mut SchemaGenerator) -> Catalog {
    let mut catalog = Catalog::default();
    handshake::register(&mut catalog, g);
    stream::register(&mut catalog, g);
    project::register(&mut catalog, g);
    session::register(&mut catalog, g);
    catalog
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Map, json};

    use super::*;
    use crate::annotations::{X_ERRORS, X_IDEMPOTENT, X_MILESTONE, X_SCOPE};
    use crate::canonical::to_canonical_string;
    use crate::error::{ErrorCode, ErrorReason};
    use crate::limits::MAX_SAFE_INTEGER;
    use crate::rpc::Scope::{self, *};
    use crate::schema::generator;
    use crate::testutil::integers;

    fn built() -> (Catalog, Map<String, Value>) {
        let mut g = generator();
        let catalog = catalog(&mut g);
        (catalog, g.take_definitions(true))
    }

    /// Requests and client notifications: what the scope check applies to.
    fn scoped(catalog: &Catalog) -> impl Iterator<Item = (&&'static str, &Value)> {
        catalog.requests.iter().chain(&catalog.client_notifications)
    }

    #[test]
    fn unit_proto_methods_core_stream_every_method_roundtrips() {
        let sampled: BTreeSet<&str> = [
            handshake::tests::samples(),
            stream::tests::samples(),
            project::tests::samples(),
            session::tests::samples(),
        ]
        .concat()
        .into_iter()
        .collect();
        let (catalog, _) = built();
        let names: BTreeSet<&str> = scoped(&catalog)
            .chain(&catalog.host_notifications)
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(sampled, names);
    }

    /// A cell of the permission matrix (design 5.8.3) as far as the
    /// method's scope decides it; the policy module adds the rest.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Cell {
        /// ✓
        Y,
        /// ✗
        N,
        /// "原生确认": the main process sends it over `control` after a native confirmation.
        Native,
        /// "仅由主进程代发": the main process sends it for the window.
        ByMain,
        /// ✗ for remote_device by SEC-9: it holds the scope, so the policy
        /// module refuses or narrows the method (design 5.8.3).
        Sec9,
    }
    use Cell::*;

    /// The columns of 5.8.3 and their scopes (design 5.8.2).
    const PRINCIPALS: [(&str, &[Scope]); 7] = [
        ("UI control", &[Read, Write, Approve, Privileged, UiLocal]),
        ("UI window", &[Read, Write, Approve, UiLocal]),
        ("cli", &[Read]),
        ("cli + write", &[Read, Write]),
        ("cli + approve", &[Read, Approve]),
        (
            "test_harness",
            &[Read, Write, Approve, Privileged, UiLocal, Test],
        ),
        ("remote_device", &[Read, Write, Approve]),
    ];

    /// A row of 5.8.3: its name there, the methods it names, its cells.
    type Row = (&'static str, &'static [&'static str], [Cell; 7]);

    /// Rows of 5.8.3, each with the methods it names. A group adds its own.
    const MATRIX: &[Row] = &[
        ("initialize", &["initialize"], [Y, ByMain, Y, Y, Y, Y, Y]),
        (
            "ping、client/presence、host/status",
            &["ping", "client/presence", "host/status"],
            [Y; 7],
        ),
        (
            "订阅、history/page、blob/get（stream/ack 属于订阅）",
            &[
                "stream/subscribe",
                "stream/unsubscribe",
                "stream/ack",
                "history/page",
                "blob/get",
            ],
            [Y; 7],
        ),
        ("blob/put", &["blob/put"], [Y, Y, N, Y, N, Y, Y]),
        (
            "host/prepare_upgrade、host/exit（有运行中的回合时原生确认）",
            &["host/prepare_upgrade", "host/exit"],
            [Native, N, N, N, N, Y, N],
        ),
        (
            "订阅、history/page、…、session/list_archived（其中 T018 的方法）",
            &[
                "agents/capabilities",
                "project/trust_info",
                "session/get",
                "session/capabilities",
                "session/list_archived",
            ],
            [Y; 7],
        ),
        (
            "project/add、project/remove（SEC-9：只能在已有项目里新建会话）",
            &["project/add", "project/remove"],
            [Y, Y, N, Y, N, Y, Sec9],
        ),
        (
            "project/trust",
            &["project/trust"],
            [Native, N, N, N, N, Y, N],
        ),
        (
            "session/set_config（非档位的配置项）、session/rename、session/archive、session/restore、agents/refresh、agents/restart（remote_device 仅 set_config 的模型和推理强度）",
            &[
                "session/set_config",
                "session/rename",
                "session/restore",
                "agents/refresh",
                "agents/restart",
            ],
            [Y, Y, N, Y, N, Y, Sec9],
        ),
        ("session/mark_read", &["session/mark_read"], [Y; 7]),
        (
            "session/create（不改变档位）、session/send、session/interrupt、session/queue_resume；session/queue_edit 按 session/send（5.3.11）",
            &[
                "session/create",
                "session/send",
                "session/interrupt",
                "session/queue_resume",
                "session/queue_edit",
            ],
            [Y, Y, N, Y, N, Y, Y],
        ),
        (
            "session/confirm_config",
            &["session/confirm_config"],
            [Native, N, N, N, N, Y, N],
        ),
    ];

    /// Checks every cell of `rows` against the method's scope; returns the
    /// methods the rows name.
    fn check_rows<'a>(
        catalog: &Catalog,
        rows: impl IntoIterator<Item = &'a Row>,
    ) -> BTreeSet<&'static str> {
        let mut covered = BTreeSet::new();
        for (row, methods, cells) in rows {
            for method in *methods {
                assert!(covered.insert(*method), "{method} in two rows");
                let entry = scoped(catalog).find(|(name, _)| *name == method);
                let scope = &entry.unwrap_or_else(|| panic!("{method} missing")).1[X_SCOPE];
                for ((principal, scopes), cell) in PRINCIPALS.iter().zip(cells) {
                    let allowed = scope == "none" || scopes.iter().any(|s| s.as_str() == scope);
                    let expected = *cell != N;
                    assert_eq!(allowed, expected, "5.8.3 {row}: {method} for {principal}");
                }
            }
        }
        covered
    }

    #[test]
    fn unit_proto_methods_core_stream_scopes_match_the_matrix() {
        let (catalog, _) = built();
        let covered = check_rows(&catalog, MATRIX);
        let all: BTreeSet<&str> = scoped(&catalog).map(|(name, _)| *name).collect();
        assert_eq!(covered, all, "every scoped method sits in one row");
    }

    #[test]
    fn unit_proto_methods_core_stream_commands_carry_a_command_id() {
        let (catalog, defs) = built();
        for (name, entry) in scoped(&catalog) {
            let params = entry["params"]["$ref"].as_str().unwrap();
            let params = &defs[params.trim_start_matches("#/$defs/")];
            let required = params["required"].as_array().cloned().unwrap_or_default();
            let has = required.contains(&json!("command_id"));
            if has {
                let command_id = &params["properties"]["command_id"];
                assert_eq!(command_id["$ref"], "#/$defs/CommandId", "{name}");
            }
            assert_eq!(has, entry[X_IDEMPOTENT] == "command", "{name}");
        }
    }

    #[test]
    fn unit_proto_methods_core_stream_milestones_and_errors_are_in_the_schema() {
        let (catalog, _) = built();
        let milestone = |name: &str| {
            let entry = scoped(&catalog)
                .chain(&catalog.host_notifications)
                .find(|(n, _)| **n == name);
            entry.unwrap().1[X_MILESTONE].clone()
        };
        let expected = [
            ("initialize", "M1a"),
            ("host/prepare_upgrade", "M1b"),
            ("ui/navigate", "M1b"),
            ("stream/ack", "M3"),
            ("stream/events", "M1a"),
        ];
        for (name, value) in expected {
            assert_eq!(milestone(name), value, "{name}");
        }
        let errors = |name: &str| catalog.requests[name][X_ERRORS].clone();
        assert_eq!(
            errors("initialize"),
            json!([
                "version_unsupported",
                "unauthenticated",
                "forbidden/already_initialized"
            ])
        );
        assert_eq!(errors("host/exit"), json!(["conflict/turn_running"]));
        assert_eq!(errors("blob/put"), json!(["too_large", "conflict"]));
        assert_eq!(errors("ping"), json!([]));
        for (name, entry) in &catalog.requests {
            for error in entry[X_ERRORS].as_array().unwrap() {
                let (code, reason) = match error.as_str().unwrap().split_once('/') {
                    Some((code, reason)) => (code, Some(reason)),
                    None => (error.as_str().unwrap(), None),
                };
                let code = ErrorCode::from(code);
                assert!(!matches!(code, ErrorCode::Unknown(_)), "{name}: {error}");
                if let Some(reason) = reason {
                    assert_eq!(
                        ErrorReason::from(reason).code(),
                        Some(code),
                        "{name}: {error}"
                    );
                }
            }
        }
    }

    #[test]
    fn unit_proto_methods_core_stream_integers_and_order_are_portable() {
        let (catalog, defs) = built();
        let mut found = Vec::new();
        let catalog = serde_json::to_value(&catalog).unwrap();
        let defs = Value::Object(defs);
        integers(&catalog, &mut found);
        integers(&defs, &mut found);
        assert!(found.len() > 20, "{}", found.len());
        let safe = MAX_SAFE_INTEGER as i64;
        for schema in found {
            assert!(schema.get("format").is_none(), "{schema}");
            let lo = schema["minimum"].as_i64().unwrap();
            let hi = schema["maximum"].as_i64().unwrap();
            assert!(-safe <= lo && hi <= safe, "{schema}");
        }

        // The tables are sorted by name whatever the registration order, and
        // the export canonicalizes to the same text however maps are ordered.
        let names: Vec<&String> = catalog["requests"].as_object().unwrap().keys().collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        let (again, _) = built();
        assert_eq!(
            to_canonical_string(&catalog).unwrap(),
            to_canonical_string(&again).unwrap()
        );
    }

    /// A method group: its `register` and its `samples`.
    type Group = (
        fn(&mut Catalog, &mut SchemaGenerator),
        fn() -> Vec<&'static str>,
    );

    /// The method groups of T018.
    const SESSION_GROUPS: [Group; 2] = [
        (project::register, project::tests::samples),
        (session::register, session::tests::samples),
    ];

    /// What `register` adds to the catalog.
    fn names(register: fn(&mut Catalog, &mut SchemaGenerator)) -> BTreeSet<&'static str> {
        let mut catalog = Catalog::default();
        register(&mut catalog, &mut generator());
        let all = scoped(&catalog).chain(&catalog.host_notifications);
        all.map(|(name, _)| *name).collect()
    }

    #[test]
    fn unit_proto_methods_core_session_every_method_roundtrips() {
        for (register, samples) in SESSION_GROUPS {
            let sampled: BTreeSet<&str> = samples().into_iter().collect();
            assert_eq!(sampled, names(register));
        }
    }

    #[test]
    fn unit_proto_methods_core_session_scopes_match_the_matrix() {
        let (catalog, _) = built();
        let ours: BTreeSet<&str> = SESSION_GROUPS.iter().flat_map(|g| names(g.0)).collect();
        let rows = MATRIX
            .iter()
            .filter(|row| row.1.iter().any(|m| ours.contains(m)));
        assert_eq!(check_rows(&catalog, rows), ours, "rows of their own");
    }

    #[test]
    fn unit_proto_methods_core_session_sec9_cells() {
        // SEC-9: a remote device adds no project and changes a session's
        // model and reasoning effort only. It holds the scopes of these
        // methods, so the policy module refuses them, not the scope check.
        let sec9: BTreeSet<&str> = MATRIX
            .iter()
            .filter(|row| row.2[6] == Sec9)
            .flat_map(|row| row.1.iter().copied())
            .collect();
        let expected = [
            "agents/refresh",
            "agents/restart",
            "project/add",
            "project/remove",
            "session/rename",
            "session/restore",
            "session/set_config",
        ];
        assert_eq!(sec9, BTreeSet::from(expected));
        let (catalog, _) = built();
        let (principal, scopes) = PRINCIPALS[6];
        assert_eq!(principal, "remote_device");
        for method in sec9 {
            let scope = &catalog.requests[method][X_SCOPE];
            assert!(scopes.iter().any(|s| s.as_str() == scope), "{method}");
        }
    }

    #[test]
    fn unit_proto_methods_core_session_milestones_and_errors_are_in_the_schema() {
        let (catalog, _) = built();
        let entry = |name: &str| &catalog.requests[name];
        for (name, milestone) in [
            ("project/add", "M1a"),
            ("project/remove", "M1b"),
            ("agents/restart", "M1b"),
            ("session/create", "M1a"),
            ("session/queue_edit", "M1b"),
            ("session/set_config", "M1b"),
            ("session/list_archived", "M1b"),
        ] {
            assert_eq!(entry(name)[X_MILESTONE], milestone, "{name}");
        }
        // 5.3.11 adds `unavailable/scan_failed`: the method scans again.
        let errors = json!(["not_found", "unavailable/scan_failed"]);
        assert_eq!(entry("project/trust_info")[X_ERRORS], errors);
        let errors = json!(["conflict/digest_mismatch", "unavailable/scan_failed"]);
        assert_eq!(entry("project/trust")[X_ERRORS], errors);
        // And `forbidden/scope`: the tier rules apply to `config` (5.8.2).
        let errors = json!(["not_found", "unavailable", "unsupported", "forbidden/scope"]);
        assert_eq!(entry("session/create")[X_ERRORS], errors);
    }
}
