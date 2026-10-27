//! Instances along the exported schema with every property filled, for
//! UNIT-proto-absent-not-null (T193). Each branch of a union is in at least
//! one of them; an `Unknown…` branch is in none. A string marked `x-id`, or
//! defined as `…Id`, is an ID with the prefix its description names first
//! (`s_` of "Session: `s_` + UUIDv7").

use serde_json::{Map, Value, json};

pub fn full(defs: &Map<String, Value>, schema: &Value) -> Vec<Value> {
    Full { defs }.instances(schema)
}

struct Full<'a> {
    defs: &'a Map<String, Value>,
}

fn list<'a>(schema: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    schema
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

impl Full<'_> {
    fn instances(&self, schema: &Value) -> Vec<Value> {
        if let Some(value) = schema.get("const") {
            return vec![value.clone()];
        }
        if let Some(first) = list(schema, "enum").next() {
            return vec![first.clone()];
        }
        let mut parts = Vec::new();
        if let Some(name) = schema.get("$ref").and_then(Value::as_str) {
            let name = name.trim_start_matches("#/$defs/");
            let def = &self.defs[name];
            if name == "StreamId" {
                return vec![json!("index")];
            }
            if name.ends_with("Id") && def["type"] == "string" {
                return vec![Value::String(string(def, true))];
            }
            parts.push(self.instances(def));
        }
        parts.extend(list(schema, "allOf").map(|part| self.instances(part)));
        let unknown = |b: &Value| b["$ref"].as_str().is_some_and(|r| r.contains("/Unknown"));
        let branches: Vec<Value> = list(schema, "anyOf")
            .chain(list(schema, "oneOf"))
            .filter(|b| !unknown(b) && b["type"] != "null")
            .flat_map(|b| self.instances(b))
            .collect();
        if !branches.is_empty() {
            parts.push(branches);
        }
        parts.extend(self.typed(schema));
        merge(parts)
    }

    fn typed(&self, schema: &Value) -> Option<Vec<Value>> {
        let kind = match &schema["type"] {
            Value::String(kind) => kind.as_str(),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .find(|k| *k != "null")?,
            _ if schema.get("properties").is_some() => "object",
            _ => return None,
        };
        let bound = |key: &str, or: i64| schema[key].as_i64().unwrap_or(or);
        let int = 1.clamp(bound("minimum", 0), bound("maximum", i64::MAX));
        Some(match kind {
            "object" => self.object(schema),
            "array" => {
                // One element, or as many as the least; each instance starts at another.
                let items = self.instances(&schema["items"]);
                let n = 1.clamp(bound("minItems", 0), bound("maxItems", i64::MAX)) as usize;
                let nth = |i: usize| (i..i + n).map(|j| items[j % items.len()].clone());
                (0..items.len())
                    .map(|i| Value::Array(nth(i).collect()))
                    .collect()
            }
            "string" => vec![Value::String(string(schema, schema.get("x-id").is_some()))],
            "integer" => vec![json!(int)],
            "number" => vec![json!(1.5)],
            "boolean" => vec![json!(true)],
            _ => return None,
        })
    }

    /// Every property, and a member of a map; the properties' instances side by side.
    fn object(&self, schema: &Value) -> Vec<Value> {
        let props = schema.get("properties").and_then(Value::as_object);
        let mut fields: Vec<(String, Vec<Value>)> = props
            .into_iter()
            .flatten()
            .map(|(key, property)| (key.clone(), self.instances(property)))
            .collect();
        if let Some(each) = schema.get("additionalProperties").filter(|s| s.is_object())
            && fields.is_empty()
        {
            fields.push(("k".into(), self.instances(each)));
        }
        let n = fields.iter().map(|(_, v)| v.len()).max().unwrap_or(1);
        let nth = |i: usize| {
            fields
                .iter()
                .map(move |(k, v)| (k.clone(), v[i % v.len()].clone()))
        };
        (0..n).map(|i| Value::Object(nth(i).collect())).collect()
    }
}

/// A string an ID, a path or any text parses from.
fn string(schema: &Value, id: bool) -> String {
    let doc = schema["description"].as_str().unwrap_or("");
    if schema.get("x-path").is_some() {
        return "/w/a".into();
    }
    if !id {
        return "x".into();
    }
    match doc.split('`').nth(1) {
        Some(named) => format!("{}01", named.find('_').map_or("", |i| &named[..=i])),
        None if doc.contains("hex") => "ab".repeat(32),
        None => "01".into(),
    }
}

/// The `i`-th instance of each part, merged into one: the members of
/// objects together, else the last part.
fn merge(parts: Vec<Vec<Value>>) -> Vec<Value> {
    if parts.is_empty() {
        return vec![json!({})];
    }
    let n = parts.iter().map(Vec::len).max().unwrap_or(1);
    let one = |i: usize| {
        let each = parts.iter().map(|p| p[i % p.len()].clone());
        each.reduce(|acc, v| match (acc, v) {
            (Value::Object(mut a), Value::Object(b)) => {
                a.extend(b);
                Value::Object(a)
            }
            (_, v) => v,
        })
    };
    (0..n).filter_map(one).collect()
}
