//! Building blocks of the exported JSON Schema (design 5.3.10): the generator
//! settings every protocol schema uses, and the per-method entries that carry
//! `x-scope`, `x-idempotent`, `x-milestone` and `x-errors`.

use schemars::generate::SchemaSettings;
use schemars::transform::RecursiveTransform;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde_json::{Value, json};

use crate::annotations::{X_ERRORS, X_IDEMPOTENT, X_MILESTONE, X_SCOPE};
use crate::limits::MAX_SAFE_INTEGER;
use crate::rpc::{ClientNotification, HostNotification, Method, MethodError, Scope};

/// Draft 2020-12, with every integer bounded by [`bound_integers`].
pub fn generator() -> SchemaGenerator {
    SchemaSettings::draft2020_12()
        .with_transform(RecursiveTransform(bound_integers))
        .into_generator()
}

/// Replaces an integer's `format` (`int64`, `uint32`, ...) with `minimum` and
/// `maximum`, never beyond plus or minus 2^53 - 1 (design 5.3.1, ADR-04), so
/// generated code uses plain numbers, never BigInt.
pub fn bound_integers(schema: &mut Schema) {
    let Some(schema) = schema.as_object_mut() else {
        return;
    };
    let integer = match schema.get("type") {
        Some(Value::String(t)) => t == "integer",
        Some(Value::Array(types)) => types.iter().any(|t| t == "integer"),
        _ => false,
    };
    if !integer {
        return;
    }
    let safe = MAX_SAFE_INTEGER as i64;
    let format = schema.remove("format");
    let (lo, hi): (i64, i64) = match format.as_ref().and_then(Value::as_str) {
        Some("int8") => (i8::MIN.into(), i8::MAX.into()),
        Some("int16") => (i16::MIN.into(), i16::MAX.into()),
        Some("int32") => (i32::MIN.into(), i32::MAX.into()),
        Some("uint8") => (0, u8::MAX.into()),
        Some("uint16") => (0, u16::MAX.into()),
        Some("uint32") => (0, u32::MAX.into()),
        Some(f) if f.starts_with("uint") => (0, safe),
        _ => (-safe, safe),
    };
    let lo = schema
        .get("minimum")
        .and_then(Value::as_i64)
        .map_or(lo, |m| m.max(lo));
    let hi = schema
        .get("maximum")
        .and_then(Value::as_i64)
        .map_or(hi, |m| m.min(hi));
    schema.insert("minimum".into(), lo.into());
    schema.insert("maximum".into(), hi.into());
}

/// `{params, result, x-scope, x-idempotent, x-milestone, x-errors}` of a request method.
pub fn method_entry<M: Method>(g: &mut SchemaGenerator) -> Value {
    let errors: Vec<String> = M::ERRORS.iter().map(MethodError::wire).collect();
    json!({
        "params": subschema::<M::Params>(g),
        "result": subschema::<M::Result>(g),
        X_SCOPE: scope_name(M::SCOPE.as_ref()),
        X_IDEMPOTENT: M::IDEMPOTENCY.as_str(),
        X_MILESTONE: M::MILESTONE.as_str(),
        X_ERRORS: errors,
    })
}

/// `{params, x-scope, x-idempotent, x-milestone}` of a client-to-host notification.
pub fn client_notification_entry<N: ClientNotification>(g: &mut SchemaGenerator) -> Value {
    json!({
        "params": subschema::<N::Params>(g),
        X_SCOPE: scope_name(N::SCOPE.as_ref()),
        X_IDEMPOTENT: N::IDEMPOTENCY.as_str(),
        X_MILESTONE: N::MILESTONE.as_str(),
    })
}

/// `{params, x-milestone}` of a host-to-client notification.
pub fn host_notification_entry<N: HostNotification>(g: &mut SchemaGenerator) -> Value {
    json!({ "params": subschema::<N::Params>(g), X_MILESTONE: N::MILESTONE.as_str() })
}

/// `x-scope`: the required scope, or `none` when any principal may call.
fn scope_name(scope: Option<&Scope>) -> &str {
    scope.map_or("none", Scope::as_str)
}

/// A subschema with the generator's transforms applied; a named type is a
/// `$ref` whose definition gets them when the definitions are taken.
fn subschema<T: JsonSchema>(g: &mut SchemaGenerator) -> Schema {
    let mut schema = g.subschema_for::<T>();
    for transform in g.transforms_mut() {
        transform.transform(&mut schema);
    }
    schema
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;
    use crate::annotations::TimestampMs;
    use crate::error::{ErrorCode, ErrorReason};
    use crate::rpc::{Idempotency, Milestone, Notification};
    use crate::testutil::integers;

    #[derive(Serialize, Deserialize, JsonSchema)]
    struct Numbers {
        seq: u64,
        after: Option<u64>,
        delta: i64,
        count: u32,
        small: u8,
        offset: i32,
        at: TimestampMs,
    }

    #[test]
    fn unit_proto_base_schema_bounds_every_integer() {
        let mut g = generator();
        let inline = subschema::<u64>(&mut g);
        g.subschema_for::<Numbers>();
        let defs = Value::Object(g.take_definitions(true));
        let props = &defs["Numbers"]["properties"];
        let safe = MAX_SAFE_INTEGER as i64;
        let bounds = |p: &Value| {
            (
                p["minimum"].as_i64().unwrap(),
                p["maximum"].as_i64().unwrap(),
            )
        };
        assert_eq!(bounds(&props["seq"]), (0, safe));
        assert_eq!(bounds(&props["after"]), (0, safe));
        assert_eq!(bounds(&props["delta"]), (-safe, safe));
        assert_eq!(bounds(&props["count"]), (0, u32::MAX.into()));
        assert_eq!(bounds(&props["small"]), (0, 255));
        assert_eq!(bounds(&props["offset"]), (i32::MIN.into(), i32::MAX.into()));
        assert_eq!(bounds(inline.as_value()), (0, safe));

        let mut found = Vec::new();
        integers(&defs, &mut found);
        assert_eq!(found.len(), 7);
        for schema in found {
            assert!(schema.get("format").is_none(), "{schema}");
            let (lo, hi) = bounds(schema);
            assert!(-safe <= lo && hi <= safe, "{schema}");
        }
    }

    struct SessionSend;
    impl Method for SessionSend {
        const NAME: &'static str = "session/send";
        const SCOPE: Option<Scope> = Some(Scope::Write);
        const IDEMPOTENCY: Idempotency = Idempotency::Command;
        const MILESTONE: Milestone = Milestone::M1b;
        const ERRORS: &'static [MethodError] = &[
            MethodError {
                code: ErrorCode::NotFound,
                reason: None,
            },
            MethodError {
                code: ErrorCode::Conflict,
                reason: Some(ErrorReason::TurnMismatch),
            },
        ];
        type Params = Numbers;
        type Result = ();
    }

    struct Presence;
    impl Notification for Presence {
        const NAME: &'static str = "client/presence";
        const MILESTONE: Milestone = Milestone::M1a;
        type Params = bool;
    }
    impl ClientNotification for Presence {
        const SCOPE: Option<Scope> = Some(Scope::Read);
        const IDEMPOTENCY: Idempotency = Idempotency::Natural;
    }

    struct Hello;
    impl Notification for Hello {
        const NAME: &'static str = "host/hello";
        const MILESTONE: Milestone = Milestone::M3;
        type Params = String;
    }
    impl HostNotification for Hello {}

    struct Initialize;
    impl Method for Initialize {
        const NAME: &'static str = "initialize";
        const SCOPE: Option<Scope> = None;
        const IDEMPOTENCY: Idempotency = Idempotency::None;
        const MILESTONE: Milestone = Milestone::M1a;
        const ERRORS: &'static [MethodError] = &[];
        type Params = ();
        type Result = ();
    }

    #[test]
    fn unit_proto_base_schema_method_annotations() {
        let mut g = generator();
        let send = method_entry::<SessionSend>(&mut g);
        assert_eq!(
            (&send[X_SCOPE], &send[X_IDEMPOTENT]),
            (&json!("write"), &json!("command"))
        );
        assert_eq!(send["params"], json!({"$ref": "#/$defs/Numbers"}));
        assert_eq!(send[X_MILESTONE], "M1b");
        assert_eq!(
            send[X_ERRORS],
            json!(["not_found", "conflict/turn_mismatch"])
        );
        let init = method_entry::<Initialize>(&mut g);
        assert_eq!(
            (&init[X_SCOPE], &init[X_IDEMPOTENT]),
            (&json!("none"), &json!("none"))
        );
        assert_eq!(init[X_ERRORS], json!([]));
        let presence = client_notification_entry::<Presence>(&mut g);
        assert_eq!(
            (&presence[X_SCOPE], &presence[X_IDEMPOTENT]),
            (&json!("read"), &json!("natural"))
        );
        assert_eq!(presence[X_MILESTONE], "M1a");
        assert_eq!(
            host_notification_entry::<Hello>(&mut g),
            json!({"params": {"type": "string"}, X_MILESTONE: "M3"})
        );
    }
}
