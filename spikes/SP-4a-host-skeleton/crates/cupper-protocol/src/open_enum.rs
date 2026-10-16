//! Open string enums: known values get variants, anything else is kept in
//! `Unknown(String)` so older readers never fail on newer values.

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
    open_enum! {
        pub enum Color { Red => "red", Blue => "blue" }
    }

    #[test]
    fn unknown_values_roundtrip() {
        let c: Color = serde_json::from_str("\"green\"").unwrap();
        assert_eq!(c, Color::Unknown("green".into()));
        assert_eq!(serde_json::to_string(&c).unwrap(), "\"green\"");
        let r: Color = serde_json::from_str("\"red\"").unwrap();
        assert_eq!(r, Color::Red);
    }
}
