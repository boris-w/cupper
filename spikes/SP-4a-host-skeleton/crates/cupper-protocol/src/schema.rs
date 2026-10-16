//! JSON Schema export: the external contract generated from the Rust types.

use schemars::generate::SchemaSettings;
use schemars::{JsonSchema, SchemaGenerator};
use serde_json::{Map, Value, json};

use crate::events::{Event, EventBody};
use crate::frame::MAX_FRAME_BYTES;
use crate::messages::*;
use crate::rpc::{Method, Notification, RpcError};

fn method<M: Method>(g: &mut SchemaGenerator, out: &mut Map<String, Value>) {
    let params = g.subschema_for::<M::Params>();
    let result = g.subschema_for::<M::Result>();
    out.insert(M::NAME.to_owned(), json!({ "params": params, "result": result }));
}

fn notification<N: Notification>(g: &mut SchemaGenerator, out: &mut Map<String, Value>) {
    let params = g.subschema_for::<N::Params>();
    out.insert(N::NAME.to_owned(), json!({ "params": params }));
}

fn def<T: JsonSchema>(g: &mut SchemaGenerator) {
    g.subschema_for::<T>();
}

/// Rebuild objects with sorted keys. serde_json's map order depends on whether
/// some crate in the build enables its `preserve_order` feature (the ACP SDK
/// does), so the exported schema must not rely on it.
fn sorted(v: Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut entries: Vec<(String, Value)> = m.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(entries.into_iter().map(|(k, v)| (k, sorted(v))).collect())
        }
        Value::Array(a) => Value::Array(a.into_iter().map(sorted).collect()),
        other => other,
    }
}

/// The whole protocol as one JSON Schema document. Method and notification
/// tables live in `x-methods` / `x-notifications`; every type is in `$defs`.
pub fn protocol_schema() -> Value {
    let mut g = SchemaSettings::draft2020_12().into_generator();
    let mut methods = Map::new();
    method::<Initialize>(&mut g, &mut methods);
    method::<Subscribe>(&mut g, &mut methods);
    method::<SessionCreate>(&mut g, &mut methods);
    method::<SessionSend>(&mut g, &mut methods);
    method::<SessionInterrupt>(&mut g, &mut methods);
    method::<InteractionRespond>(&mut g, &mut methods);
    method::<BlobGet>(&mut g, &mut methods);
    let mut notifications = Map::new();
    notification::<StreamEvents>(&mut g, &mut notifications);
    notification::<StreamSnapshot>(&mut g, &mut notifications);
    notification::<StreamReset>(&mut g, &mut notifications);
    def::<RpcError>(&mut g);
    def::<Event>(&mut g);
    def::<EventBody>(&mut g);
    let defs = g.take_definitions(true);
    sorted(json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "cupper host protocol",
        "x-protocol-version": PROTOCOL_VERSION,
        "x-max-frame-bytes": MAX_FRAME_BYTES,
        "x-mergeable-event-types": EventBody::MERGEABLE_TYPES,
        "x-methods": methods,
        "x-notifications": notifications,
        "$defs": defs,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The committed schema must match the Rust types. Run with
    /// `UPDATE_SCHEMA=1` to regenerate it.
    #[test]
    fn committed_schema_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../protocol/schema")
            .join(format!("v{PROTOCOL_VERSION}.json"));
        let generated = serde_json::to_string_pretty(&protocol_schema()).unwrap() + "\n";
        if std::env::var_os("UPDATE_SCHEMA").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &generated).unwrap();
        }
        let committed = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(committed == generated, "schema is stale; rerun with UPDATE_SCHEMA=1");
    }
}
