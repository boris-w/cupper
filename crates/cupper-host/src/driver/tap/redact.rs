//! Structured redaction of raw frames (design 5.11.2): account fields and
//! credentials in known formats, nothing else. Raw frames must stay usable for
//! recomputing history, so the log redaction's long-string rule does not apply
//! and every other byte of a line is kept.

use std::borrow::Cow;

use serde_json::{Map, Value, json};

/// The extension notification that carries the login state (K16).
pub(crate) const AUTH_STATUS_METHOD: &str = "_auth/status_update";

/// Replaces the secret part of a credential.
const REDACTED: &str = "<redacted>";

/// Prefixes of common API keys and tokens; what follows them is the secret.
const KEY_PREFIXES: &[&str] = &[
    "sk-",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "glpat-",
    "xoxb-",
    "xoxp-",
    "AKIA",
];

/// A prefix followed by fewer characters than this is a word, not a key.
const MIN_KEY_LEN: usize = 16;

/// A redacted copy of an `_auth/status_update` notification that keeps only
/// the login kind and the plan name (`kind`, `label`, `account.plan`); `None`
/// for any other message.
pub(crate) fn account_fields(msg: &Value) -> Option<Value> {
    if msg.get("method")?.as_str()? != AUTH_STATUS_METHOD {
        return None;
    }
    let status = msg.pointer("/params/authStatus").and_then(Value::as_object);
    let mut kept = Map::new();
    for key in ["kind", "label"] {
        if let Some(value) = status.and_then(|status| status.get(key)) {
            kept.insert(key.to_owned(), value.clone());
        }
    }
    if let Some(plan) = status.and_then(|status| status.get("account")?.get("plan")) {
        kept.insert("account".to_owned(), json!({ "plan": plan }));
    }
    let mut msg = msg.clone();
    msg["params"] = json!({ "authStatus": kept });
    Some(msg)
}

/// `text` with every credential in a known format masked: API keys by
/// prefix, JWTs, `Bearer` tokens, and the userinfo and query of URLs.
pub(crate) fn credentials(text: &str) -> Cow<'_, str> {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut copied = 0;
    let mut at = 0;
    while at < bytes.len() {
        match secret_at(text, at) {
            Some((end, masked)) => {
                out.push_str(&text[copied..at]);
                out.push_str(&masked);
                copied = end;
                at = end;
            }
            None => at += 1,
        }
    }
    if copied == 0 {
        return Cow::Borrowed(text);
    }
    out.push_str(&text[copied..]);
    Cow::Owned(out)
}

/// A credential starting at byte `at`: where it ends and what replaces it.
/// Every format starts with an ASCII letter where a token can start, so base64
/// data and the middle of identifiers never match.
fn secret_at(text: &str, at: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    if !bytes[at].is_ascii_alphabetic() || !at_boundary(bytes, at) {
        return None;
    }
    let rest = &text[at..];
    for prefix in KEY_PREFIXES {
        if rest.starts_with(prefix) {
            let len = run(bytes, at + prefix.len(), is_key_char);
            if len >= MIN_KEY_LEN {
                return Some((at + prefix.len() + len, format!("{prefix}{REDACTED}")));
            }
        }
    }
    for scheme in ["Bearer ", "bearer "] {
        if rest.starts_with(scheme) {
            let len = run(bytes, at + scheme.len(), |b| {
                is_key_char(b) || matches!(b, b'.' | b'~' | b'+' | b'/' | b'=')
            });
            if len >= 8 {
                return Some((at + scheme.len() + len, format!("{scheme}{REDACTED}")));
            }
        }
    }
    jwt_at(bytes, at).or_else(|| url_at(text, at))
}

/// `header.payload.signature`, each base64url, the header starting `eyJ`.
fn jwt_at(bytes: &[u8], at: usize) -> Option<(usize, String)> {
    if !bytes[at..].starts_with(b"eyJ") {
        return None;
    }
    let mut end = at;
    for (part, min) in [10, 10, 0].into_iter().enumerate() {
        if part > 0 {
            if bytes.get(end) != Some(&b'.') {
                return None;
            }
            end += 1;
        }
        let len = run(bytes, end, is_key_char);
        if len < min {
            return None;
        }
        end += len;
    }
    Some((end, REDACTED.to_owned()))
}

/// A URL with userinfo or a query, rewritten without them.
fn url_at(text: &str, at: usize) -> Option<(usize, String)> {
    let bytes = text.as_bytes();
    let scheme = run(bytes, at, |b| {
        b.is_ascii_alphanumeric() || b"+-.".contains(&b)
    });
    if !text[at + scheme..].starts_with("://") {
        return None;
    }
    let start = at + scheme + 3;
    let end = start
        + run(bytes, start, |b| {
            !b.is_ascii_whitespace() && !b"\"'\\<>`".contains(&b)
        });
    let (authority, rest) = text[start..end].split_at(
        text[start..end]
            .find(['/', '?', '#'])
            .unwrap_or(end - start),
    );
    let userinfo = authority.rfind('@');
    let query = rest.find('?');
    if userinfo.is_none() && query.is_none() {
        return None;
    }
    let mut out = text[at..start].to_owned();
    match userinfo {
        Some(sign) => out.extend([REDACTED, &authority[sign..]]),
        None => out.push_str(authority),
    }
    match query {
        Some(mark) => {
            let fragment = rest[mark..]
                .find('#')
                .map_or(rest.len(), |hash| mark + hash);
            out.extend([&rest[..=mark], REDACTED, &rest[fragment..]]);
        }
        None => out.push_str(rest),
    }
    Some((end, out))
}

/// Whether a token can start at `at`: the byte before is not part of a word,
/// or ends a JSON escape such as `\n`.
fn at_boundary(bytes: &[u8], at: usize) -> bool {
    match at.checked_sub(1).map(|before| bytes[before]) {
        None => true,
        Some(b) if !is_key_char(b) && !matches!(b, b'.' | b'+' | b'/') => true,
        Some(b) => at >= 2 && bytes[at - 2] == b'\\' && b"nrtbf".contains(&b),
    }
}

fn is_key_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// How many bytes from `from` on satisfy `pred`.
fn run(bytes: &[u8], from: usize, pred: impl Fn(u8) -> bool) -> usize {
    bytes[from..].iter().take_while(|b| pred(**b)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_tap_redaction_keeps_only_the_kind_and_plan_of_the_login() {
        // The shape claude-agent-acp 0.91.0 sends (SP-1, values replaced).
        let msg = json!({"jsonrpc": "2.0", "method": "_auth/status_update", "params": {
            "authStatus": {"kind": "account", "label": "Claude Max",
                "account": {"plan": "max", "email": "someone@example.com", "organization": "Example Org"}},
            "_meta": {"account": "someone@example.com"}}});
        let redacted = account_fields(&msg).unwrap();
        assert_eq!(
            redacted["params"],
            json!({"authStatus": {"kind": "account", "label": "Claude Max", "account": {"plan": "max"}}})
        );
        assert!(!redacted.to_string().contains("example"));

        let none =
            json!({"method": AUTH_STATUS_METHOD, "params": {"authStatus": {"kind": "none"}}});
        assert_eq!(
            account_fields(&none).unwrap()["params"],
            json!({"authStatus": {"kind": "none"}})
        );
        assert_eq!(
            account_fields(&json!({"method": "session/update", "params": {}})),
            None
        );
    }

    #[test]
    fn unit_tap_redaction_masks_credentials_in_known_formats() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.c2lnbmF0dXJl";
        for (input, expected) in [
            (
                r#"{"k":"sk-ant-api03-AbCdEf0123456789xyz"}"#,
                r#"{"k":"sk-<redacted>"}"#,
            ),
            (
                "export TOKEN=ghp_0123456789abcdefABCDEF",
                "export TOKEN=ghp_<redacted>",
            ),
            (
                r#""line one\nAKIAABCDEFGHIJKLMNOP""#,
                r#""line one\nAKIA<redacted>""#,
            ),
            (
                "Authorization: Bearer abc.def-123_456",
                "Authorization: Bearer <redacted>",
            ),
            (&format!("token {jwt} end"), "token <redacted> end"),
            (
                "https://user:pw@example.com/a/b?x=1&y=2#top",
                "https://<redacted>@example.com/a/b?<redacted>#top",
            ),
            (
                r#"{"uri":"git+ssh://git@host/repo"}"#,
                r#"{"uri":"git+ssh://<redacted>@host/repo"}"#,
            ),
            (
                "密钥 sk-ant-0123456789abcdefghij 结束",
                "密钥 sk-<redacted> 结束",
            ),
        ] {
            assert_eq!(credentials(input), expected, "{input}");
        }
    }

    #[test]
    fn unit_tap_redaction_leaves_everything_else_alone() {
        for text in [
            r#"{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"0199a1b2-c3d4"}}"#,
            // base64 image data, including a stretch that looks like a key or a JWT
            "iVBORw0KGgoAAAANSUhEUgAAAAEAAAAB+sk-0123456789abcdefgh/eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIx.c2ln",
            "task-runner sk-short ask-0123456789abcdef0123 https://example.com/plain/path",
            "Bearer short and a bare eyJ mention",
        ] {
            assert!(matches!(credentials(text), Cow::Borrowed(_)), "{text}");
        }
    }
}
