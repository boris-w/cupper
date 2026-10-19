//! Open unions (design 5.3.1, 5.3.11): `type`-tagged unions that accept members
//! this version does not know, while still rejecting known members whose data
//! is malformed.
//!
//! serde's derived `#[serde(tag = ...)]` with a trailing `#[serde(untagged)]`
//! fallback cannot tell the two apart: a known tag with malformed data quietly
//! lands in the fallback. [`open_union!`](crate::open_union!) derives
//! `Serialize` and `JsonSchema` as usual (the schema is an `anyOf`), and
//! generates `Deserialize`: a known tag parses strictly as its member and
//! errors propagate; only an unknown tag becomes `Unknown`, with its data kept.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A member of an internally tagged open union whose tag this version does
/// not know: the whole object, tag included, kept as is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct UnknownMember(pub Map<String, Value>);

impl UnknownMember {
    /// The value of the tag field `field`.
    pub fn tag(&self, field: &str) -> &str {
        self.0
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default()
    }
}

/// The string value of the tag field `field`, for the generated `Deserialize`.
#[doc(hidden)]
pub fn tag_of(object: &Map<String, Value>, field: &str) -> Result<String, String> {
    match object.get(field) {
        Some(Value::String(tag)) => Ok(tag.clone()),
        _ => Err(format!("missing or non-string `{field}`")),
    }
}

/// Declares an open union: an enum whose members are newtypes of a struct or
/// unit variants, plus a generated `Unknown` member.
///
/// - `in tag "<field>"`: internally tagged; the tag is a field of the member's
///   object and `Unknown` holds an [`UnknownMember`].
/// - `in tag "type", content "data"`: adjacently tagged, as event bodies are
///   (design 5.3.5); `Unknown` holds an [`UnknownEvent`](crate::envelope::UnknownEvent).
///
/// Variant attributes, such as docs or `#[schemars(extend("x-mergeable" = true))]`,
/// are kept. The enum gets `TAGS` (the known tags) and `tag()`.
#[macro_export]
macro_rules! open_union {
    (
        $(#[$meta:meta])*
        pub enum $name:ident in tag "type", content "data" {
            $( $(#[$vmeta:meta])* $variant:ident $(($ty:ty))? => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::schemars::JsonSchema)]
        #[serde(tag = "type", content = "data")]
        pub enum $name {
            $( $(#[$vmeta])* #[serde(rename = $wire)] $variant $(($ty))?, )+
            /// A type this version does not know, kept as is.
            #[serde(untagged)]
            Unknown($crate::envelope::UnknownEvent),
        }

        $crate::open_union!(@common $name { $($variant => $wire),+ } |unknown| unknown.kind.as_str());

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                use ::serde::de::Error as _;
                let mut object = ::serde_json::Map::<String, ::serde_json::Value>::deserialize(d)?;
                let tag = $crate::open_union::tag_of(&object, "type").map_err(D::Error::custom)?;
                let data = object.remove("data").unwrap_or_default();
                let member = match tag.as_str() {
                    $( $wire => $crate::open_union!(@member $variant $(($ty))? data), )+
                    _ => return Ok(Self::Unknown($crate::envelope::UnknownEvent { kind: tag, data })),
                };
                member.map_err(|e| D::Error::custom(format_args!("`{tag}`: {e}")))
            }
        }
    };
    (
        $(#[$meta:meta])*
        pub enum $name:ident in tag $tag:literal {
            $( $(#[$vmeta:meta])* $variant:ident $(($ty:ty))? => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::schemars::JsonSchema)]
        #[serde(tag = $tag)]
        pub enum $name {
            $( $(#[$vmeta])* #[serde(rename = $wire)] $variant $(($ty))?, )+
            /// A member this version does not know, kept as is.
            #[serde(untagged)]
            Unknown($crate::open_union::UnknownMember),
        }

        $crate::open_union!(@common $name { $($variant => $wire),+ } |unknown| unknown.tag($tag));

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                use ::serde::de::Error as _;
                let object = ::serde_json::Map::<String, ::serde_json::Value>::deserialize(d)?;
                let tag = $crate::open_union::tag_of(&object, $tag).map_err(D::Error::custom)?;
                let member = match tag.as_str() {
                    $( $wire => $crate::open_union!(@member $variant $(($ty))? ::serde_json::Value::Object(object)), )+
                    _ => return Ok(Self::Unknown($crate::open_union::UnknownMember(object))),
                };
                member.map_err(|e| D::Error::custom(format_args!("`{tag}`: {e}")))
            }
        }
    };
    (@common $name:ident { $($variant:ident => $wire:literal),+ } |$unknown:ident| $unknown_tag:expr) => {
        impl $name {
            /// Tags this version knows.
            pub const TAGS: &'static [&'static str] = &[$($wire),+];

            /// The tag of this member, known or not.
            pub fn tag(&self) -> &str {
                match self {
                    $( Self::$variant { .. } => $wire, )+
                    Self::Unknown($unknown) => $unknown_tag,
                }
            }
        }
    };
    (@member $variant:ident ($ty:ty) $value:expr) => {
        ::serde_json::from_value::<$ty>($value).map(Self::$variant)
    };
    (@member $variant:ident $value:expr) => {{
        let _ = $value;
        Ok::<Self, ::serde_json::Error>(Self::$variant)
    }};
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::rpc::Scope;

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
    pub struct Dot {
        scope: Scope,
        size: u32,
    }

    open_union! {
        /// A union tagged by `kind`, with a unit member.
        pub enum Shape in tag "kind" {
            /// A dot.
            Dot(Dot) => "dot",
            Empty => "empty",
        }
    }

    fn parse(v: Value) -> Result<Shape, serde_json::Error> {
        serde_json::from_value(v)
    }

    #[test]
    fn unit_proto_events_index_open_union_known_and_unknown_members() {
        let dot = json!({"kind": "dot", "scope": "teal", "size": 2});
        let parsed = parse(dot.clone()).unwrap();
        assert_eq!(
            parsed,
            Shape::Dot(Dot {
                scope: Scope::Unknown("teal".into()),
                size: 2
            })
        );
        assert_eq!(serde_json::to_value(&parsed).unwrap(), dot);

        // A unit member ignores other fields and serializes as its tag alone.
        let empty = parse(json!({"kind": "empty", "extra": 1})).unwrap();
        assert_eq!(empty, Shape::Empty);
        assert_eq!(
            serde_json::to_value(&empty).unwrap(),
            json!({"kind": "empty"})
        );

        // An unknown tag keeps the whole object.
        let cube = json!({"kind": "cube", "edge": [1, 2], "scope": "read"});
        let parsed = parse(cube.clone()).unwrap();
        assert!(matches!(&parsed, Shape::Unknown(_)));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), cube);

        assert_eq!(Shape::TAGS, ["dot", "empty"]);
        let tags: Vec<String> = [dot, json!({"kind": "empty"}), cube]
            .into_iter()
            .map(|v| parse(v).unwrap().tag().to_owned())
            .collect();
        assert_eq!(tags, ["dot", "empty", "cube"]);
    }

    #[test]
    fn unit_proto_events_index_open_union_rejects_malformed_known_members() {
        for (bad, why) in [
            (
                json!({"kind": "dot", "scope": "read"}),
                "missing field `size`",
            ),
            (
                json!({"kind": "dot", "scope": "read", "size": -1}),
                "invalid value",
            ),
            (
                json!({"scope": "read", "size": 1}),
                "missing or non-string `kind`",
            ),
            (json!({"kind": 3}), "missing or non-string `kind`"),
        ] {
            let err = parse(bad.clone()).unwrap_err().to_string();
            assert!(err.contains(why), "{bad}: {err}");
        }
        let err = parse(json!({"kind": "dot", "size": 1})).unwrap_err();
        assert!(err.to_string().starts_with("`dot`: "), "{err}");
    }

    #[test]
    fn unit_proto_events_index_open_union_schema_lists_members() {
        let schema = serde_json::to_value(schemars::schema_for!(Shape)).unwrap();
        let members = schema["anyOf"].as_array().unwrap();
        let tags: Vec<&Value> = members
            .iter()
            .filter_map(|m| m.pointer("/properties/kind/const"))
            .collect();
        assert_eq!(tags, ["dot", "empty"]);
        assert_eq!(members.len(), 3, "the known members plus the fallback");
        assert_eq!(members[0]["description"], "A dot.");
    }
}
