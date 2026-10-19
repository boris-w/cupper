//! Open types (design 5.3.1): enums and `type`-tagged unions accept values this
//! version does not know, so older readers never fail on newer values.
//!
//! - Enums: [`open_enum!`](crate::open_enum) keeps unknown strings in `Unknown(String)`.
//! - Internally tagged unions (`#[serde(tag = "type")]`) end with an
//!   `#[serde(untagged)] Unknown(UnknownTagged)` member.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Fallback member of an internally tagged union: a `type` this version does
/// not know, with the rest of the object kept as is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnknownTagged {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// Declares an open string enum with serde and JSON Schema support.
///
/// The schema is `anyOf: [{enum: [...known]}, {type: string}]`, so generated
/// validators accept unknown values while still documenting the known ones.
#[macro_export]
macro_rules! open_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident => $wire:literal ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant, )+
            /// A value this version does not know.
            Unknown(String),
        }

        impl $name {
            pub fn as_str(&self) -> &str {
                match self {
                    $( $name::$variant => $wire, )+
                    $name::Unknown(s) => s.as_str(),
                }
            }
        }

        impl ::std::convert::From<&str> for $name {
            fn from(s: &str) -> Self {
                match s {
                    $( $wire => $name::$variant, )+
                    other => $name::Unknown(other.to_owned()),
                }
            }
        }

        impl ::std::fmt::Display for $name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl ::serde::Serialize for $name {
            fn serialize<S: ::serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <::std::borrow::Cow<'de, str> as ::serde::Deserialize>::deserialize(d)?;
                Ok($name::from(s.as_ref()))
            }
        }

        impl ::schemars::JsonSchema for $name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }

            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!({
                    "description": concat!("Open enum: unknown values must be accepted. Known: ", $($wire, " "),+),
                    "anyOf": [
                        { "type": "string", "enum": [$($wire),+] },
                        { "type": "string" }
                    ]
                })
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    open_enum! {
        pub enum Color { Red => "red", Blue => "blue" }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    #[serde(tag = "type")]
    enum Shape {
        #[serde(rename = "dot")]
        Dot { color: Color },
        #[serde(untagged)]
        Unknown(UnknownTagged),
    }

    #[test]
    fn unit_proto_base_open_enum_unknown_values_roundtrip() {
        let c: Color = serde_json::from_str("\"green\"").unwrap();
        assert_eq!(c, Color::Unknown("green".into()));
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"green\"");
        assert_eq!(
            serde_json::from_str::<Color>("\"red\"").unwrap(),
            Color::Red
        );
        assert_eq!(Color::from("blue").as_str(), "blue");
    }

    #[test]
    fn unit_proto_base_open_union_unknown_members_roundtrip() {
        let raw = r#"{"type":"cube","edge":2,"color":"red"}"#;
        let s: Shape = serde_json::from_str(raw).unwrap();
        let Shape::Unknown(u) = &s else {
            panic!("{s:?}")
        };
        assert_eq!(u.kind, "cube");
        assert_eq!(
            serde_json::to_value(&s).unwrap(),
            serde_json::from_str::<Value>(raw).unwrap()
        );
        let dot: Shape = serde_json::from_str(r#"{"type":"dot","color":"teal"}"#).unwrap();
        assert_eq!(
            dot,
            Shape::Dot {
                color: Color::Unknown("teal".into())
            }
        );
    }
}
