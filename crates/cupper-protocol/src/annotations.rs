//! Schema annotations (design 5.3.10): what the policy layer and golden-test
//! normalization (design 5.9.3) need to know about a method, an event or a field.
//!
//! | Key | Marks | Comes from |
//! |---|---|---|
//! | `x-scope`, `x-idempotent` | every method | the `SCOPE` and `IDEMPOTENCY` constants of [`Method`](crate::rpc::Method) and [`ClientNotification`](crate::rpc::ClientNotification), written by [`crate::schema`] |
//! | `x-milestone` | every method and notification | the `MILESTONE` constant of [`Method`](crate::rpc::Method) and [`Notification`](crate::rpc::Notification): `M1a`, `M1b`, `M2` or `M3` |
//! | `x-errors` | every method | the `ERRORS` constant of [`Method`](crate::rpc::Method): `code` or `code/reason`, besides the errors of every method |
//! | `x-mergeable` | event types that send-time coalescing may merge | `#[schemars(extend("x-mergeable" = true))]` on the event variant, next to its [`Mergeable`](crate::coalesce::Mergeable) rule |
//! | `x-id` | values that differ from run to run: IDs, command IDs, digests | the types in [`crate::ids`] |
//! | `x-time` | timestamps and durations | [`TimestampMs`], [`DurationMs`] |
//! | `x-path` | filesystem paths | [`WirePath`] |
//!
//! Field annotations ride on the field's type, so a new field of one of these
//! types is covered without anything else to remember. A field that has to
//! keep a plain type carries the key itself, e.g. `#[schemars(extend("x-path" = true))]`.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};

use crate::limits::MAX_SAFE_INTEGER;

pub const X_SCOPE: &str = "x-scope";
pub const X_IDEMPOTENT: &str = "x-idempotent";
pub const X_MILESTONE: &str = "x-milestone";
pub const X_ERRORS: &str = "x-errors";
pub const X_MERGEABLE: &str = "x-mergeable";
pub const X_ID: &str = "x-id";
pub const X_TIME: &str = "x-time";
pub const X_PATH: &str = "x-path";

/// UTC milliseconds since the Unix epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TimestampMs(pub u64);

/// A duration in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DurationMs(pub u64);

/// A filesystem path as the protocol carries it: UTF-8 text, absolute or
/// relative to the workspace as the field documents.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WirePath(pub String);

fn millis(description: &str) -> Schema {
    json_schema!({
        "type": "integer",
        "minimum": 0,
        "maximum": MAX_SAFE_INTEGER,
        "description": description,
        X_TIME: true,
    })
}

impl JsonSchema for TimestampMs {
    fn schema_name() -> Cow<'static, str> {
        "TimestampMs".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        millis("UTC milliseconds since the Unix epoch.")
    }
}

impl JsonSchema for DurationMs {
    fn schema_name() -> Cow<'static, str> {
        "DurationMs".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        millis("A duration in milliseconds.")
    }
}

impl JsonSchema for WirePath {
    fn schema_name() -> Cow<'static, str> {
        "WirePath".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "type": "string",
            "description": "A filesystem path: absolute, or relative to the workspace.",
            X_PATH: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use schemars::generate::SchemaSettings;
    use serde_json::Value;

    use super::*;
    use crate::ids::{CommandId, PrincipalId, SessionId};

    #[derive(JsonSchema)]
    #[expect(dead_code, reason = "only the schema is under test")]
    struct Fields {
        session: SessionId,
        command: Option<CommandId>,
        principal: PrincipalId,
        at: TimestampMs,
        took: Option<DurationMs>,
        paths: Vec<WirePath>,
        #[schemars(extend("x-path" = true))]
        cwd: String,
    }

    #[derive(JsonSchema)]
    #[serde(tag = "type", content = "data")]
    #[expect(dead_code, reason = "only the schema is under test")]
    enum Body {
        #[serde(rename = "chunk")]
        #[schemars(extend("x-mergeable" = true))]
        Chunk(String),
        #[serde(rename = "end")]
        End(u32),
    }

    fn annotated(defs: &serde_json::Map<String, Value>, name: &str, key: &str) -> bool {
        defs[name].get(key) == Some(&Value::Bool(true))
    }

    #[test]
    fn unit_proto_base_annotations_follow_field_types() {
        let mut g = SchemaSettings::draft2020_12().into_generator();
        let fields = g.subschema_for::<Fields>();
        let body = g.subschema_for::<Body>();
        let defs = g.take_definitions(true);

        for id in ["SessionId", "CommandId"] {
            assert!(annotated(&defs, id, X_ID), "{id}");
        }
        assert!(
            !annotated(&defs, "PrincipalId", X_ID),
            "principal IDs are stable"
        );
        assert!(annotated(&defs, "TimestampMs", X_TIME));
        assert!(annotated(&defs, "DurationMs", X_TIME));
        assert!(annotated(&defs, "WirePath", X_PATH));
        assert_eq!(fields.get("$ref").unwrap(), "#/$defs/Fields");
        assert_eq!(defs["Fields"]["properties"]["cwd"][X_PATH], true);

        let variants = defs["Body"]["oneOf"].as_array().unwrap();
        let mergeable: Vec<&Value> = variants
            .iter()
            .filter(|v| v.get(X_MERGEABLE) == Some(&Value::Bool(true)))
            .map(|v| &v["properties"]["type"]["const"])
            .collect();
        assert_eq!(mergeable, ["chunk"]);
        assert_eq!(body.get("$ref").unwrap(), "#/$defs/Body");
    }
}
