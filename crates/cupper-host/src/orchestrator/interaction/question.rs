//! A form becomes a question, and its answer goes back as the form's content
//! (design 5.3.6).
//!
//! - Claude's AskUserQuestion asks each question as `question_N` (its
//!   choices) beside `question_N_custom` (free text). The pair is one field
//!   with `allow_custom`, labelled with the question's text; an answer that
//!   is not one of the choices goes back in `question_N_custom`.
//! - Any other property maps by its JSON Schema type: a string with `oneOf`
//!   or `enum` is a single choice, an array of those a multiple choice.
//! - A form with anything else (a nested object, an array of free strings, a
//!   `required` that names no property) is `unsupported_schema`, without
//!   fields: the client offers only "skip".

use std::collections::BTreeMap;

use cupper_protocol::interaction::{
    Answer, AnswerValue, Choice, FieldType, QuestionField, QuestionRequest,
};
use serde_json::{Map, Value};

use crate::driver::api::FormAsk;

/// Where each field of a question goes back in the form, by field index.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Layout(Vec<Slot>);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Slot {
    /// Claude's `question_N_custom`, which takes an answer beyond the choices.
    custom: Option<String>,
    /// A JSON Schema `integer`: the answer must be a whole number.
    integer: bool,
}

/// Why an answer does not fit its request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AnswerError {
    #[error("the outcome does not answer this request")]
    Outcome,
    #[error("the question has no field {0}")]
    UnknownField(String),
    #[error("required field {0} is not answered")]
    Missing(String),
    #[error("the answer to {0} does not fit the field")]
    Mismatch(String),
}

/// The question `form` asks, and where its answers go back.
pub fn map(form: &FormAsk) -> (QuestionRequest, Layout) {
    let fields = fields(&form.schema);
    let unsupported_schema = fields.is_none();
    let (fields, slots) = fields.unwrap_or_default().into_iter().unzip();
    let question = QuestionRequest {
        prompt: form.message.clone(),
        fields,
        unsupported_schema,
    };
    (question, Layout(slots))
}

fn fields(schema: &Value) -> Option<Vec<(QuestionField, Slot)>> {
    if schema.get("type").is_some_and(|t| t != "object") {
        return None;
    }
    let properties = schema.get("properties")?.as_object()?;
    let required: Vec<&str> = match schema.get("required") {
        None => Vec::new(),
        Some(list) => list
            .as_array()?
            .iter()
            .map(Value::as_str)
            .collect::<Option<_>>()?,
    };
    if !required.iter().all(|key| properties.contains_key(*key)) {
        return None;
    }
    let paired = |key: &str| pair(properties, key, &required);
    let mut fields = Vec::new();
    for (key, property) in properties {
        if key.strip_suffix("_custom").is_some_and(paired) {
            continue;
        }
        let (field_type, choices) = shape(property)?;
        let custom = paired(key).then(|| format!("{key}_custom"));
        let text = |name| property.get(name).and_then(Value::as_str);
        // Claude puts the question in `description` and its header in `title`.
        let label = match custom {
            Some(_) => text("description").or(text("title")),
            None => text("title").or(text("description")),
        };
        let field = QuestionField {
            field_id: key.clone(),
            label: label.unwrap_or(key).to_owned(),
            field_type,
            choices,
            allow_custom: custom.is_some(),
            required: required.contains(&key.as_str()),
        };
        let integer = property["type"] == "integer";
        fields.push((field, Slot { custom, integer }));
    }
    Some(fields)
}

/// Claude's pair: `key` has choices, and `{key}_custom` is an optional plain string.
fn pair(properties: &Map<String, Value>, key: &str, required: &[&str]) -> bool {
    let custom = format!("{key}_custom");
    let shape_of = |key: &str| properties.get(key).and_then(shape);
    let plain = matches!(shape_of(&custom), Some((FieldType::Text, None)));
    let choice = shape_of(key).is_some_and(|(_, choices)| choices.is_some());
    plain && choice && !required.contains(&custom.as_str())
}

/// A property's type and choices; `None` when a field cannot say it.
fn shape(property: &Value) -> Option<(FieldType, Option<Vec<Choice>>)> {
    Some(match property.get("type")?.as_str()? {
        "string" => match choices(property)? {
            Some(choices) => (FieldType::SingleChoice, Some(choices)),
            None => (FieldType::Text, None),
        },
        "array" => (
            FieldType::MultiChoice,
            Some(choices(property.get("items")?)??),
        ),
        "number" | "integer" => (FieldType::Number, None),
        "boolean" => (FieldType::Boolean, None),
        _ => return None,
    })
}

/// The `oneOf` or `anyOf` options (`{const, title?, description?}`), or the
/// `enum` strings, of `schema`: `Some(None)` without any, `None` when malformed.
fn choices(schema: &Value) -> Option<Option<Vec<Choice>>> {
    let text = |value: &Value, name| value.get(name).and_then(Value::as_str).map(str::to_owned);
    let choices: Vec<Choice> = if let Some(list) = schema.get("oneOf").or(schema.get("anyOf")) {
        let option = |option: &Value| {
            let value = text(option, "const")?;
            let label = text(option, "title").unwrap_or_else(|| value.clone());
            let description = text(option, "description");
            Some(Choice {
                value,
                label,
                description,
            })
        };
        list.as_array()?.iter().map(option).collect::<Option<_>>()?
    } else if let Some(list) = schema.get("enum") {
        let value = |value: &Value| value.as_str().map(str::to_owned);
        let values: Vec<String> = list.as_array()?.iter().map(value).collect::<Option<_>>()?;
        let choice = |value: String| Choice {
            label: value.clone(),
            value,
            description: None,
        };
        values.into_iter().map(choice).collect()
    } else {
        return Some(None);
    };
    (!choices.is_empty()).then_some(Some(choices))
}

/// Checks `answer` against `question` and lays it out as the form's content
/// (ACP's `accept`), keyed by the form's own property names.
pub fn merge(
    question: &QuestionRequest,
    layout: &Layout,
    answer: &Answer,
) -> Result<Answer, AnswerError> {
    if question.unsupported_schema {
        return Err(AnswerError::Outcome);
    }
    let known = |id: &&String| question.fields.iter().any(|field| field.field_id == **id);
    if let Some(id) = answer.values.keys().find(|id| !known(id)) {
        return Err(AnswerError::UnknownField(id.clone()));
    }
    let mut values = BTreeMap::new();
    for (index, field) in question.fields.iter().enumerate() {
        let id = &field.field_id;
        let Some(value) = answer.values.get(id) else {
            if field.required {
                return Err(AnswerError::Missing(id.clone()));
            }
            continue;
        };
        let slot = layout.0.get(index).cloned().unwrap_or_default();
        let custom = slot.custom.unwrap_or_else(|| id.clone());
        let listed = |value: &String| field.choices.iter().flatten().any(|c| c.value == *value);
        let mismatch = Err(AnswerError::Mismatch(id.clone()));
        match (&field.field_type, value) {
            (FieldType::SingleChoice, AnswerValue::Text(text)) if listed(text) => {
                values.insert(id.clone(), value.clone());
            }
            (FieldType::SingleChoice, AnswerValue::Text(_)) if field.allow_custom => {
                values.insert(custom, value.clone());
            }
            (FieldType::MultiChoice, AnswerValue::Choices(items)) => {
                let (picks, mut other): (Vec<String>, _) = items.iter().cloned().partition(listed);
                if other.len() > usize::from(field.allow_custom) {
                    return mismatch;
                }
                if custom == *id {
                    values.insert(custom, value.clone());
                    continue;
                }
                if !picks.is_empty() {
                    values.insert(id.clone(), AnswerValue::Choices(picks));
                }
                if let Some(text) = other.pop() {
                    values.insert(custom, AnswerValue::Text(text));
                }
            }
            (FieldType::Text, AnswerValue::Text(_))
            | (FieldType::Boolean, AnswerValue::Boolean(_)) => {
                values.insert(id.clone(), value.clone());
            }
            (FieldType::Number, AnswerValue::Number(n)) if !slot.integer || !n.is_f64() => {
                values.insert(id.clone(), value.clone());
            }
            _ => return mismatch,
        }
    }
    Ok(Answer { values })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn form(schema: Value) -> (QuestionRequest, Layout) {
        let message = "Asked".into();
        map(&FormAsk { message, schema })
    }

    /// Each field as `id: label, type[, custom][, required]`.
    fn shape(question: &QuestionRequest) -> Vec<String> {
        let field = |f: &QuestionField| {
            let custom = if f.allow_custom { ", custom" } else { "" };
            let required = if f.required { ", required" } else { "" };
            format!(
                "{}: {}, {}{custom}{required}",
                f.field_id, f.label, f.field_type
            )
        };
        question.fields.iter().map(field).collect()
    }

    /// Merges `values` and returns the form's content as JSON.
    fn merged(form: &(QuestionRequest, Layout), values: Value) -> Result<Value, AnswerError> {
        let answer = serde_json::from_value(json!({ "values": values })).unwrap();
        let answer = merge(&form.0, &form.1, &answer)?;
        Ok(serde_json::to_value(answer.values).unwrap())
    }

    #[test]
    fn unit_question_mapping_claude_question_and_its_custom_box_are_one_field() {
        // s05-questions, claude-agent-acp 0.91.0.
        let asked = form(json!({"type": "object", "properties": {
            "question_0": {"type": "string", "title": "Color", "oneOf": [
                {"const": "red", "title": "red", "description": "Choose red"},
                {"const": "blue", "title": "blue", "description": "Choose blue"}]},
            "question_0_custom": {"type": "string", "title": "Other",
                "description": "Type your own answer, or add a note to the option you chose above (optional)."}}}));
        assert_eq!(
            shape(&asked.0),
            ["question_0: Color, single_choice, custom"]
        );
        let choices = serde_json::to_value(&asked.0.fields[0].choices).unwrap();
        let red = json!({"value": "red", "label": "red", "description": "Choose red"});
        assert_eq!(choices[0], red);
        assert!(!asked.0.unsupported_schema);
        let red = merged(&asked, json!({"question_0": "red"}));
        assert_eq!(red, Ok(json!({"question_0": "red"})));
        let custom = merged(&asked, json!({"question_0": "green"}));
        assert_eq!(custom, Ok(json!({"question_0_custom": "green"})));
        assert_eq!(merged(&asked, json!({})), Ok(json!({})));
    }

    #[test]
    fn unit_question_mapping_claude_questions_split_and_merge_back() {
        let custom = json!({"type": "string", "title": "Other"});
        let asked = form(json!({"type": "object", "properties": {
            "question_0": {"type": "string", "title": "Color", "description": "Which color?",
                "oneOf": [{"const": "red", "title": "red"}, {"const": "blue", "title": "blue"}]},
            "question_0_custom": custom,
            "question_1": {"type": "array", "title": "Sizes", "description": "Which sizes?",
                "items": {"anyOf": [{"const": "s", "title": "Small"}, {"const": "m", "title": "M"}]}},
            "question_1_custom": custom}}));
        // The question's text labels each field; the header is only a title.
        let expected = [
            "question_0: Which color?, single_choice, custom",
            "question_1: Which sizes?, multi_choice, custom",
        ];
        assert_eq!(shape(&asked.0), expected);
        let both = json!({"question_0": "blue", "question_1": ["s", "XL", "m"]});
        let content =
            json!({"question_0": "blue", "question_1": ["s", "m"], "question_1_custom": "XL"});
        assert_eq!(merged(&asked, both), Ok(content));
        let only_custom = merged(&asked, json!({"question_1": ["XL"]}));
        assert_eq!(only_custom, Ok(json!({"question_1_custom": "XL"})));
    }

    #[test]
    fn unit_question_mapping_other_forms_map_by_schema_type() {
        // s05-questions, codex-acp 2.2.2, with a field of each other type.
        let asked = form(
            json!({"type": "object", "required": ["color"], "properties": {
            "color": {"type": "string", "title": "Pick a color", "description": "Color",
                "_meta": {"codex": {"isOther": true}},
                "oneOf": [{"const": "Red (Recommended)", "title": "Red (Recommended)"}]},
            "color_note": {"type": "string", "title": "Additional answer or note"},
            "size": {"type": "string", "enum": ["s", "m"]},
            "count": {"type": "integer"}, "ratio": {"type": "number"}, "ok": {"type": "boolean"}}}),
        );
        let mut fields = shape(&asked.0);
        fields.sort();
        let expected = [
            "color: Pick a color, single_choice, required",
            "color_note: Additional answer or note, text",
            "count: count, number",
            "ok: ok, boolean",
            "ratio: ratio, number",
            "size: size, single_choice",
        ];
        assert_eq!(fields, expected);
        let values = json!({"color": "Red (Recommended)", "color_note": "ok", "size": "m",
            "count": 3, "ratio": 2.5, "ok": true});
        assert_eq!(merged(&asked, values.clone()), Ok(values));
    }

    #[test]
    fn unit_question_mapping_unsupported_schema_offers_only_skip() {
        let with = |property: Value| json!({"type": "object", "properties": {"q": property}});
        for schema in [
            json!({"type": "array", "properties": {}}),
            json!({"type": "object", "properties": []}),
            json!({"type": "object"}),
            json!({"type": "object", "required": ["gone"], "properties": {}}),
            json!({"type": "object", "required": "q", "properties": {"q": {"type": "boolean"}}}),
            with(json!({"type": "object", "properties": {}})),
            with(json!({"type": "array", "items": {"type": "string"}})),
            with(json!({"type": "array"})),
            with(json!({"title": "no type"})),
            with(json!({"type": ["string", "null"]})),
            with(json!({"type": "string", "oneOf": [{"const": 1}]})),
            with(json!({"type": "string", "oneOf": []})),
            with(json!({"type": "string", "enum": ["a", 2]})),
        ] {
            let asked = form(schema.clone());
            let (prompt, fields) = (asked.0.prompt.as_str(), &asked.0.fields);
            assert!(asked.0.unsupported_schema && fields.is_empty() && prompt == "Asked");
            assert_eq!(
                merged(&asked, json!({})),
                Err(AnswerError::Outcome),
                "{schema}"
            );
        }
    }

    #[test]
    fn unit_question_mapping_answers_must_fit_their_fields() {
        let choice = json!({"type": "string", "enum": ["a", "b"]});
        let asked = form(
            json!({"type": "object", "required": ["one"], "properties": {
            "one": choice, "many": {"type": "array", "items": choice},
            "many_custom": {"type": "string"}, "plain": {"type": "array", "items": choice},
            "text": {"type": "string"}, "count": {"type": "integer"}, "ok": {"type": "boolean"}}}),
        );
        let refused = |values: Value| merged(&asked, values).unwrap_err().to_string();
        let mismatch = |id| format!("the answer to {id} does not fit the field");
        assert_eq!(
            refused(json!({"one": "a", "x": 1})),
            "the question has no field x"
        );
        assert_eq!(
            refused(json!({"text": "x"})),
            "required field one is not answered"
        );
        for (id, value) in [
            ("one", json!("c")),
            ("one", json!(["a"])),
            ("many", json!(["x", "y"])),
            ("plain", json!(["a", "x"])),
            ("text", json!(true)),
            ("count", json!(2.5)),
            ("count", json!("2")),
            ("ok", json!("yes")),
        ] {
            let mut values = json!({"one": "a"});
            values[id] = value;
            assert_eq!(refused(values), mismatch(id), "{id}");
        }
        let fits = json!({"one": "a", "plain": ["b", "a"], "count": -2});
        assert_eq!(merged(&asked, fits.clone()), Ok(fits));
    }
}
