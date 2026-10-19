//! Canonical JSON (RFC 8785, JCS) and `d_` digests (design 5.3.1, 5.4.4).
//!
//! Every digest in the protocol (an interaction request's `digest`, the
//! ledger's `params_hash`, the `config_digest` a user confirms) is the full
//! sha-256 of the JCS form. Rust, TypeScript and Swift must agree byte for byte:
//!
//! - object members are sorted by the UTF-16 code units of their names;
//! - strings escape only `"`, `\` and control characters (`\u00xx` in lowercase);
//! - numbers are written the way ECMAScript writes an IEEE 754 double;
//! - integers beyond plus or minus 2^53 - 1 are refused: the protocol has none,
//!   and JavaScript cannot hold them exactly, so the digests would differ.

use serde::Serialize;
use serde_json::{Number, Value};
use sha2::{Digest as _, Sha256};
use thiserror::Error;

use crate::ids::Digest;
use crate::limits::MAX_SAFE_INTEGER;

#[derive(Debug, Error)]
pub enum CanonicalError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("integer {0} is beyond the protocol's 2^53 - 1 bound")]
    UnsafeInteger(Number),
}

/// The RFC 8785 form of `value`.
pub fn to_canonical_string<T: Serialize + ?Sized>(value: &T) -> Result<String, CanonicalError> {
    let mut out = String::new();
    write_value(&mut out, &serde_json::to_value(value)?, None)?;
    Ok(out)
}

/// The RFC 8785 form of `value` indented by two spaces, with a final newline,
/// for committed files people review: dropping the whitespace between tokens
/// gives [`to_canonical_string`] back.
pub fn to_canonical_pretty<T: Serialize + ?Sized>(value: &T) -> Result<String, CanonicalError> {
    let mut out = String::new();
    write_value(&mut out, &serde_json::to_value(value)?, Some(0))?;
    out.push('\n');
    Ok(out)
}

/// `d_` + the sha-256 of the RFC 8785 form of `value`.
pub fn digest<T: Serialize + ?Sized>(value: &T) -> Result<Digest, CanonicalError> {
    let canonical = to_canonical_string(value)?;
    Ok(Digest::from_sha256(
        &Sha256::digest(canonical.as_bytes()).into(),
    ))
}

/// Writes `value`; `depth` is the nesting level when indenting, `None` for none.
fn write_value(
    out: &mut String,
    value: &Value,
    depth: Option<usize>,
) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(out, n)?,
        Value::String(s) => write_string(out, s),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                separate(out, i, depth);
                write_value(out, item, depth.map(|d| d + 1))?;
            }
            close(out, items.is_empty(), depth, ']');
        }
        Value::Object(members) => {
            let mut members: Vec<(&String, &Value)> = members.iter().collect();
            members.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push('{');
            for (i, (name, item)) in members.iter().enumerate() {
                separate(out, i, depth);
                write_string(out, name);
                out.push_str(if depth.is_some() { ": " } else { ":" });
                write_value(out, item, depth.map(|d| d + 1))?;
            }
            close(out, members.is_empty(), depth, '}');
        }
    }
    Ok(())
}

/// Before the `i`th element: a comma after the first, and when indenting a
/// new line at one level deeper than `depth`.
fn separate(out: &mut String, i: usize, depth: Option<usize>) {
    if i > 0 {
        out.push(',');
    }
    if let Some(depth) = depth {
        out.push('\n');
        out.push_str(&"  ".repeat(depth + 1));
    }
}

/// The closing bracket, on a line of its own after a non-empty container when indenting.
fn close(out: &mut String, empty: bool, depth: Option<usize>, bracket: char) {
    if let (Some(depth), false) = (depth, empty) {
        out.push('\n');
        out.push_str(&"  ".repeat(depth));
    }
    out.push(bracket);
}

fn write_number(out: &mut String, n: &Number) -> Result<(), CanonicalError> {
    if n.is_f64() {
        write_double(out, n.as_f64().expect("a float number has an f64 value"));
        return Ok(());
    }
    match n.as_i64() {
        Some(i) if i.unsigned_abs() <= MAX_SAFE_INTEGER => out.push_str(&i.to_string()),
        _ => return Err(CanonicalError::UnsafeInteger(n.clone())),
    }
    Ok(())
}

/// ECMAScript's Number::toString for a finite double (ECMA-262, Number::toString),
/// which RFC 8785 adopts.
fn write_double(out: &mut String, value: f64) {
    if value == 0.0 {
        out.push('0'); // Also minus zero.
        return;
    }
    if value < 0.0 {
        out.push('-');
    }
    // value = digits * 10^(n - k), as in the ECMAScript algorithm.
    let (digits, n) = shortest_digits(value.abs());
    let k = digits.len() as i32;
    let zeros = |count: i32| "0".repeat(count as usize);
    if k <= n && n <= 21 {
        out.push_str(&digits);
        out.push_str(&zeros(n - k));
    } else if 0 < n && n <= 21 {
        let (int, frac) = digits.split_at(n as usize);
        out.push_str(&format!("{int}.{frac}"));
    } else if -6 < n && n <= 0 {
        out.push_str(&format!("0.{}{digits}", zeros(-n)));
    } else {
        let (first, rest) = digits.split_at(1);
        let dot = if rest.is_empty() { "" } else { "." };
        let sign = if n > 0 { '+' } else { '-' };
        out.push_str(&format!("{first}{dot}{rest}e{sign}{}", (n - 1).abs()));
    }
}

/// The fewest significant digits that read back as `value`, and the exponent
/// `n` such that `value = 0.digits * 10^n`.
fn shortest_digits(value: f64) -> (String, i32) {
    let split = |sci: &str| {
        let (mantissa, exponent) = sci.split_once('e').expect("`{:e}` writes an exponent");
        let exponent = exponent
            .parse::<i32>()
            .expect("`{:e}` writes an integer exponent");
        (mantissa.replace('.', ""), exponent + 1)
    };
    // `{:e}` gives the fewest digits that read back as `value`, but of two such
    // digit strings equally close to `value` it may take the odd one, where
    // ECMAScript takes the even one. Rounding `value` to that many digits
    // (ties to even) gives the closest candidate; keep it if it reads back.
    let shortest = format!("{value:e}");
    let places = split(&shortest).0.len() - 1;
    let rounded = format!("{value:.places$e}");
    if rounded.parse::<f64>() == Ok(value) {
        split(&rounded)
    } else {
        split(&shortest)
    }
}

fn write_string(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < '\u{20}' => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use serde_json::json;

    use super::*;

    fn jcs(text: &str) -> String {
        to_canonical_string(&serde_json::from_str::<Value>(text).unwrap()).unwrap()
    }

    #[test]
    fn unit_digest_jcs_rfc8785_example() {
        // RFC 8785 section 3.2.2 and its canonical form in 3.2.3.
        let input = r#"{
            "numbers": [333333333.33333329, 1E30, 4.50, 2e-3, 0.000000000000000000000000001],
            "string": "\u20ac$\u000F\u000aA'\u0042\u0022\u005c\\\"\/",
            "literals": [null, true, false]
        }"#;
        let expected = r#"{"literals":[null,true,false],"numbers":[333333333.3333333,1e+30,4.5,0.002,1e-27],"string":"€$\u000f\nA'B\"\\\\\"/"}"#;
        assert_eq!(jcs(input), expected);
        assert_eq!(jcs(expected), expected, "canonical form is a fixed point");
    }

    #[test]
    fn unit_digest_jcs_sorts_names_by_utf16() {
        // RFC 8785 section 3.2.3: U+1F600 (a surrogate pair) sorts before U+FB33.
        let input = r#"{"\u20ac":"Euro Sign","\r":"Carriage Return","\ufb33":"Hebrew Letter Dalet With Dagesh",
            "1":"One","\ud83d\ude00":"Emoji: Grinning Face","\u0080":"Control","\u00f6":"Latin Small Letter O With Diaeresis"}"#;
        let order = [
            "Carriage Return",
            "One",
            "Control",
            "Latin Small Letter O With Diaeresis",
            "Euro Sign",
            "Emoji: Grinning Face",
            "Hebrew Letter Dalet With Dagesh",
        ];
        let text = jcs(input);
        let positions: Vec<usize> = order.iter().map(|v| text.find(v).unwrap()).collect();
        assert!(positions.is_sorted(), "{text}");
    }

    #[test]
    fn unit_digest_jcs_string_escapes() {
        let s = "\u{0}\u{8}\u{9}\u{a}\u{c}\u{d}\u{1f}\u{7f}\u{80}\u{2028}é😀\"\\/";
        let expected = "\"\\u0000\\b\\t\\n\\f\\r\\u001f\u{7f}\u{80}\u{2028}é😀\\\"\\\\/\"";
        assert_eq!(to_canonical_string(s).unwrap(), expected);
    }

    #[test]
    fn unit_digest_jcs_numbers() {
        // RFC 8785 appendix B (NaN and Infinity cannot occur in a serde_json value).
        let samples = [
            (0x0000000000000000, "0"),
            (0x8000000000000000, "0"),
            (0x0000000000000001, "5e-324"),
            (0x8000000000000001, "-5e-324"),
            (0x7fefffffffffffff, "1.7976931348623157e+308"),
            (0xffefffffffffffff, "-1.7976931348623157e+308"),
            (0x4340000000000000, "9007199254740992"),
            (0xc340000000000000, "-9007199254740992"),
            (0x4430000000000000, "295147905179352830000"),
            (0x44b52d02c7e14af5, "9.999999999999997e+22"),
            (0x44b52d02c7e14af6, "1e+23"),
            (0x44b52d02c7e14af7, "1.0000000000000001e+23"),
            (0x444b1ae4d6e2ef4e, "999999999999999700000"),
            (0x444b1ae4d6e2ef4f, "999999999999999900000"),
            (0x444b1ae4d6e2ef50, "1e+21"),
            (0x3eb0c6f7a0b5ed8c, "9.999999999999997e-7"),
            (0x3eb0c6f7a0b5ed8d, "0.000001"),
            (0x41b3de4355555553, "333333333.3333332"),
            (0x41b3de4355555554, "333333333.33333325"),
            (0x41b3de4355555555, "333333333.3333333"),
            (0x41b3de4355555556, "333333333.3333334"),
            (0x41b3de4355555557, "333333333.33333343"),
            (0xbecbf647612f3696, "-0.0000033333333333333333"),
            (0x43143ff3c1cb0959, "1424953923781206.2"),
        ];
        for (bits, expected) in samples {
            let value = f64::from_bits(bits);
            assert_eq!(
                to_canonical_string(&value).unwrap(),
                expected,
                "{bits:016x}"
            );
        }
        assert_eq!(
            to_canonical_string(&[1.0, 1.5, -2.0]).unwrap(),
            "[1,1.5,-2]"
        );
    }

    #[test]
    fn unit_proto_base_integers_beyond_2_53_are_refused() {
        let max = MAX_SAFE_INTEGER;
        assert_eq!(to_canonical_string(&max).unwrap(), "9007199254740991");
        assert_eq!(
            to_canonical_string(&-(max as i64)).unwrap(),
            "-9007199254740991"
        );
        for bad in [
            json!(max + 1),
            json!(-(max as i64) - 1),
            json!(u64::MAX),
            json!(i64::MIN),
        ] {
            assert!(
                matches!(
                    to_canonical_string(&bad),
                    Err(CanonicalError::UnsafeInteger(_))
                ),
                "{bad}"
            );
        }
        assert!(digest(&json!({"seq": max + 1})).is_err());
    }

    #[test]
    fn unit_digest_jcs_digest_is_full_sha256() {
        #[derive(Serialize)]
        struct Request {
            b: u32,
            a: &'static str,
        }
        let d = digest(&Request { b: 1, a: "x" }).unwrap();
        // sha-256 of `{"a":"x","b":1}`.
        let expected = "d_cdab067e9f3beb32d1252cfd63e492592fecbf591b0d08cadb24bb17f3864246";
        assert_eq!(d.as_str(), expected);
        assert_eq!(digest(&json!({"a": "x", "b": 1})).unwrap(), d);
    }

    #[test]
    fn unit_proto_fixtures_schema_file_is_canonical_json_indented() {
        let value = json!({"b": [1.5, {}, []], "a": {"é": "x\ny", "A": null}});
        let pretty = to_canonical_pretty(&value).unwrap();
        let expected = "{\n  \"a\": {\n    \"A\": null,\n    \"é\": \"x\\ny\"\n  },\n  \
                        \"b\": [\n    1.5,\n    {},\n    []\n  ]\n}\n";
        assert_eq!(pretty, expected);
        let compact: String = pretty
            .split('\n')
            .map(|line| line.trim_start().replacen("\": ", "\":", 1))
            .collect();
        assert_eq!(compact, to_canonical_string(&value).unwrap());
        assert!(to_canonical_pretty(&json!(u64::MAX)).is_err());
    }

    proptest! {
        #[test]
        fn unit_digest_jcs_doubles_read_back_exactly(bits in any::<u64>()) {
            let value = f64::from_bits(bits);
            prop_assume!(value.is_finite());
            let text = to_canonical_string(&value).unwrap();
            prop_assert_eq!(text.parse::<f64>().unwrap(), if value == 0.0 { 0.0 } else { value });
        }
    }
}
