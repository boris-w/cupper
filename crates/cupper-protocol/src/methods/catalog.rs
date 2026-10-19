//! Every method and notification by name, as schema entries (design 5.3.4,
//! 5.3.10): what the schema export writes, and what the tests below check
//! method by method. A new group adds one `register` line to [`catalog`].

use std::collections::BTreeMap;

use schemars::SchemaGenerator;
use serde::Serialize;
use serde_json::Value;

use super::{handshake, stream};
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
        let sampled: BTreeSet<&str> = [handshake::tests::samples(), stream::tests::samples()]
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

    /// Rows of 5.8.3, each with the methods it names. A group adds its own.
    const MATRIX: &[(&str, &[&str], [Cell; 7])] = &[
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
    ];

    #[test]
    fn unit_proto_methods_core_stream_scopes_match_the_matrix() {
        let (catalog, _) = built();
        let mut covered = BTreeSet::new();
        for (row, methods, cells) in MATRIX {
            for method in *methods {
                assert!(covered.insert(*method), "{method} in two rows");
                let entry = scoped(&catalog).find(|(name, _)| *name == method);
                let scope = &entry.unwrap_or_else(|| panic!("{method} missing")).1[X_SCOPE];
                for ((principal, scopes), cell) in PRINCIPALS.iter().zip(cells) {
                    let allowed = scope == "none" || scopes.iter().any(|s| s.as_str() == scope);
                    let expected = *cell != N;
                    assert_eq!(allowed, expected, "5.8.3 {row}: {method} for {principal}");
                }
            }
        }
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
}
