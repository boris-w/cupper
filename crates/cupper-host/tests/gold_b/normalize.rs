//! The normalization of B-layer goldens (design 5.9.3). What a run puts in
//! a session's stream differs from run to run in ways no golden can hold,
//! so before the comparison:
//!
//! - consecutive `item.delta`s of one item and field are folded into one,
//!   since where the text is cut depends on the coalescing and the load;
//! - `seq` and `from` are dropped: an event's place is its line;
//! - what the schema marks (`cupper_protocol::annotations`) is replaced:
//!   an `x-id` by `<prefix>_<n>` in the order the values appear, counting per
//!   prefix (`s_1`, `t_1`, `d_1`; `id_<n>` without one); an `x-path` by
//!   `<path_n>` in the same way; an `x-time` by `<time>`, which keeps only
//!   whether there was one;
//! - keys are sorted, one event per line.
//!
//! The marks come from the schema itself (`schema::document`), so a new field
//! of a marked type is covered without anything else to change.

use std::collections::HashMap;

use cupper_protocol::annotations::{X_ID, X_PATH, X_TIME};
use cupper_protocol::schema;
use serde_json::{Map, Value, json};

pub struct Normalizer {
    defs: Map<String, Value>,
    /// Placeholder of each value seen, and the count of each prefix.
    seen: HashMap<String, String>,
    counts: HashMap<String, u32>,
}

impl Normalizer {
    pub fn new() -> Self {
        let Value::Object(defs) = schema::document()["$defs"].take() else {
            panic!("the schema has $defs")
        };
        Self {
            defs,
            seen: HashMap::new(),
            counts: HashMap::new(),
        }
    }

    /// Events, as the log has them, as golden lines.
    pub fn events(&mut self, events: Vec<Value>) -> Vec<String> {
        let event = json!({"$ref": "#/$defs/Event"});
        let mut lines = Vec::new();
        for mut folded in fold(events) {
            self.walk(&event, &mut folded);
            lines.push(sorted(&folded).to_string());
        }
        lines
    }

    fn walk(&mut self, schema: &Value, value: &mut Value) {
        if self.mark(schema, value) {
            return;
        }
        // A union's branch may be a `$ref` beside the tag's own properties.
        if let Some(target) = schema["$ref"].as_str() {
            let resolved = self.def(target);
            self.walk(&resolved, value);
        }
        for key in ["anyOf", "oneOf"] {
            if let Some(branches) = schema[key].as_array()
                && let Some(branch) = self.branch(branches, value)
            {
                self.walk(&branch, value);
            }
        }
        for part in schema["allOf"].as_array().into_iter().flatten() {
            self.walk(part, value);
        }
        match value {
            Value::Object(map) => {
                let properties = schema["properties"].as_object();
                for (key, field) in map.iter_mut() {
                    match properties.and_then(|p| p.get(key)) {
                        Some(sub) => self.walk(sub, field),
                        None if schema["additionalProperties"].is_object() => {
                            self.walk(&schema["additionalProperties"], field)
                        }
                        None => {}
                    }
                }
            }
            Value::Array(items) if schema["items"].is_object() => {
                for item in items {
                    self.walk(&schema["items"], item);
                }
            }
            _ => {}
        }
    }

    /// Replaces `value` when `schema` marks it; whether it did.
    fn mark(&mut self, schema: &Value, value: &mut Value) -> bool {
        let marked = |key: &str| schema[key] == true;
        let replaced = match &*value {
            Value::Null => return false,
            _ if marked(X_TIME) => "<time>".to_owned(),
            Value::String(text) if marked(X_ID) => self.placeholder(text, id_prefix(text)),
            Value::String(text) if marked(X_PATH) => {
                format!("<{}>", self.placeholder(text, "path"))
            }
            _ => return false,
        };
        *value = Value::String(replaced);
        true
    }

    fn placeholder(&mut self, text: &str, prefix: &str) -> String {
        let key = format!("{prefix}\u{0}{text}");
        if let Some(known) = self.seen.get(&key) {
            return known.clone();
        }
        let count = self.counts.entry(prefix.to_owned()).or_default();
        *count += 1;
        let new = format!("{prefix}_{count}");
        self.seen.insert(key, new.clone());
        new
    }

    fn def(&self, target: &str) -> Value {
        self.defs[target.trim_start_matches("#/$defs/")].clone()
    }

    /// The branch of a union that `value` takes: the one whose constants it
    /// has (a tag, beside a `$ref` or in it), else the first that is not
    /// `null`.
    fn branch(&self, branches: &[Value], value: &Value) -> Option<Value> {
        let consts = |node: &Value| -> Option<bool> {
            let properties = node["properties"].as_object()?;
            let consts: Vec<_> = (properties.iter())
                .filter_map(|(key, p)| Some((key, p.get("const")?)))
                .collect();
            let matches = consts.iter().all(|(key, c)| value.get(*key) == Some(*c));
            (!consts.is_empty()).then_some(matches)
        };
        let tagged = |branch: &Value| {
            let resolved = branch["$ref"].as_str().map(|target| self.def(target));
            consts(branch).or_else(|| consts(resolved.as_ref()?))
        };
        if let Some(branch) = branches.iter().find(|b| tagged(b) == Some(true)) {
            return Some(branch.clone());
        }
        let untagged = branches.iter().filter(|b| tagged(b).is_none());
        untagged.into_iter().find(|b| b["type"] != "null").cloned()
    }
}

/// `s` of `s_01…`; `id` for a value without a short lowercase prefix.
fn id_prefix(text: &str) -> &str {
    match text.split_once('_') {
        Some((prefix, _)) if (1..=4).contains(&prefix.len()) => {
            match prefix.bytes().all(|b| b.is_ascii_lowercase()) {
                true => prefix,
                false => "id",
            }
        }
        _ => "id",
    }
}

/// Consecutive `item.delta`s of one item and field, folded into the first;
/// every event without `seq` and `from`.
fn fold(events: Vec<Value>) -> Vec<Value> {
    let delta = |e: &Value| {
        let data = &e["data"];
        (e["type"] == "item.delta").then(|| (data["item_id"].clone(), data["field"].clone()))
    };
    let mut folded: Vec<Value> = Vec::new();
    for mut event in events {
        if let Some(last) = folded.last_mut()
            && delta(&event).is_some()
            && delta(last) == delta(&event)
        {
            let text = |e: &Value| e["data"]["text"].as_str().unwrap_or_default().to_owned();
            last["data"]["text"] = Value::String(text(last) + &text(&event));
            continue;
        }
        let envelope = event.as_object_mut().expect("an event is an object");
        envelope.retain(|key, _| key != "seq" && key != "from");
        folded.push(event);
    }
    folded
}

/// `value` with every object's keys in order.
pub fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            Value::Object(
                keys.into_iter()
                    .map(|k| (k.clone(), sorted(&map[k])))
                    .collect(),
            )
        }
        Value::Array(items) => Value::Array(items.iter().map(sorted).collect()),
        other => other.clone(),
    }
}

/// The first field where `actual` differs from `golden`, as `a.b[2].c`, and
/// both values there.
pub fn first_difference(golden: &Value, actual: &Value) -> Option<(String, Value, Value)> {
    fn at(path: &str, key: &str) -> String {
        match path {
            "" => key.to_owned(),
            _ => format!("{path}.{key}"),
        }
    }
    fn walk(path: String, golden: &Value, actual: &Value) -> Option<(String, Value, Value)> {
        match (golden, actual) {
            (Value::Object(g), Value::Object(a)) => {
                let mut keys: Vec<&String> = g.keys().chain(a.keys()).collect();
                keys.sort();
                keys.dedup();
                keys.into_iter()
                    .find_map(|key| match (g.get(key), a.get(key)) {
                        (Some(g), Some(a)) => walk(at(&path, key), g, a),
                        (g, a) => {
                            let (g, a) = (g.cloned(), a.cloned());
                            Some((at(&path, key), g.unwrap_or_default(), a.unwrap_or_default()))
                        }
                    })
            }
            (Value::Array(g), Value::Array(a)) if g.len() == a.len() => (g.iter().zip(a))
                .enumerate()
                .find_map(|(n, (g, a))| walk(format!("{path}[{n}]"), g, a)),
            _ if golden == actual => None,
            _ => Some((path, golden.clone(), actual.clone())),
        }
    }
    walk(String::new(), golden, actual)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn normalized(bodies: Value) -> Vec<Value> {
        let Value::Array(bodies) = bodies else {
            panic!("an array")
        };
        let lines = Normalizer::new().events(bodies);
        lines
            .iter()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn turn(id: &str, no: u32) -> Value {
        json!({"type": "turn.started", "data": {"turn_id": id, "turn_no": no, "origin": "user",
            "started_at": 1_792_000_000_000_u64}})
    }

    #[test]
    fn gold_b_fake_normalize_x_id_by_prefix_in_order() {
        let item = json!({"type": "item.started", "data": {"item_id": "i_9", "turn_id": "t_b",
            "started_at": 1, "kind": "agent_message"}});
        let events = normalized(json!([turn("t_b", 7), turn("t_a", 8), item]));
        // The same value, the same placeholder; each prefix counts on its own.
        assert_eq!(events[0]["data"]["turn_id"], "t_1");
        assert_eq!(events[1]["data"]["turn_id"], "t_2");
        assert_eq!(events[2]["data"]["turn_id"], "t_1");
        assert_eq!(events[2]["data"]["item_id"], "i_1");
        // Not marked: a number and a plain string stay.
        assert_eq!(events[0]["data"]["turn_no"], 7);
        assert_eq!(events[0]["data"]["origin"], "user");
        assert_eq!(id_prefix("0193-uuid"), "id");
        assert_eq!(id_prefix("Abc_1"), "id");
    }

    #[test]
    fn gold_b_fake_normalize_x_time_keeps_only_that_there_was_one() {
        let ended = json!({"type": "turn.completed", "data": {"turn_id": "t_1",
            "status": "completed", "ended_at": 5, "duration_ms": 0}});
        let events = normalized(json!([turn("t_1", 3), ended]));
        assert_eq!(events[0]["data"]["started_at"], "<time>");
        assert_eq!(events[1]["data"]["ended_at"], "<time>");
        assert_eq!(events[1]["data"]["duration_ms"], "<time>");
        assert_eq!(events[0]["data"]["turn_no"], 3);
        assert_eq!(events[1]["data"]["status"], "completed");
    }

    #[test]
    fn gold_b_fake_normalize_x_path_in_order() {
        let ready = |path: &str| json!({"type": "workspace.ready", "data": {"path": path, "branch": "/a/b"}});
        let events = normalized(json!([ready("/a/b"), ready("/c"), ready("/a/b")]));
        let paths: Vec<&Value> = events.iter().map(|e| &e["data"]["path"]).collect();
        assert_eq!(
            paths,
            [&json!("<path_1>"), &json!("<path_2>"), &json!("<path_1>")]
        );
        // A branch is no path, whatever it looks like.
        assert_eq!(events[0]["data"]["branch"], "/a/b");
        // Inside the item union, flattened into `item.started`.
        let tool = json!({"type": "item.started", "data": {"item_id": "i_1", "started_at": 1,
            "kind": "tool_call", "tool_kind": "execute", "title": "/c", "cwd": "/c"}});
        let tool = &normalized(json!([tool]))[0]["data"];
        assert_eq!(
            (&tool["cwd"], &tool["title"]),
            (&json!("<path_1>"), &json!("/c"))
        );
    }

    #[test]
    fn gold_b_fake_normalize_folds_deltas_and_sorts_keys() {
        let delta = |item: &str, text: &str| {
            json!({"type": "item.delta", "data": {"item_id": item, "field": "text", "text": text},
                "seq": 3, "ts": 4})
        };
        let bodies = vec![delta("i_a", "he"), delta("i_a", "llo"), delta("i_b", "!")];
        let lines = Normalizer::new().events(bodies);
        assert_eq!(
            lines,
            [
                r#"{"data":{"field":"text","item_id":"i_1","text":"hello"},"ts":"<time>","type":"item.delta"}"#,
                r#"{"data":{"field":"text","item_id":"i_2","text":"!"},"ts":"<time>","type":"item.delta"}"#,
            ]
        );
        let (golden, actual) = (json!({"a": [1, {"b": 2}]}), json!({"a": [1, {"b": 3}]}));
        let (path, ..) = first_difference(&golden, &actual).unwrap();
        assert_eq!(path, "a[1].b");
        assert_eq!(first_difference(&golden, &golden), None);
    }
}
