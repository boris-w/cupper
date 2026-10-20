//! Full redaction (design 5.11.2), the tier for logs and diagnostics: the
//! structured tier's credential rules, then the home directory (`~`), the user
//! name (`<user>`), the per-user temporary directory (`<tmpdir>`), the host
//! name (`<host>`), and every letters-and-digits string of 40 characters or
//! more (`<redacted>`). The host's own hashes and IDs are let through that
//! last rule by field name.

use super::redact::{REDACTED, credentials};

/// Strings at least this long that mix letters and digits are taken for
/// keys, tokens or encoded data.
const LONG: usize = 40;

/// What identifies this machine and its user in logs.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    home: Option<String>,
    user: Option<String>,
    /// Longest first, so `name.local` goes before `name`.
    hosts: Vec<String>,
}

impl Redactor {
    /// `home` such as `/Users/name`; the user name is its last component.
    pub fn new(home: Option<&str>, host: Option<&str>) -> Self {
        let home = home
            .filter(|home| home.len() > 1)
            .map(|home| home.trim_end_matches('/'));
        let user = home
            .and_then(|home| home.rsplit('/').next())
            .filter(|user| !user.is_empty());
        let mut hosts: Vec<String> = host
            .into_iter()
            .flat_map(|host| [host, host.split('.').next().unwrap_or(host)])
            .filter(|host| !host.is_empty())
            .map(str::to_owned)
            .collect();
        hosts.dedup();
        Self {
            home: home.map(str::to_owned),
            user: user.map(str::to_owned),
            hosts,
        }
    }

    /// The running user's `$HOME` and this machine's host name.
    pub fn for_this_machine() -> Self {
        let home = std::env::var("HOME").ok();
        let host = cupper_sys::host_name().ok();
        Self::new(home.as_deref(), host.as_deref())
    }

    /// `value`, the value of a field called `field`, redacted in full; a field
    /// that holds one of the host's own IDs or hashes keeps its long strings.
    pub fn field(&self, field: &str, value: &str) -> String {
        let own = field == "id"
            || ["_id", "hash", "digest", "blob"]
                .iter()
                .any(|end| field.ends_with(end));
        self.redact(value, !own)
    }

    /// `text` redacted in full.
    pub fn text(&self, text: &str) -> String {
        self.redact(text, true)
    }

    fn redact(&self, text: &str, long: bool) -> String {
        let mut text = credentials(text).into_owned();
        if let Some(home) = &self.home {
            text = replace(&text, home, "~", |b| !is_word(b));
        }
        text = temp_dirs(&text);
        if let Some(user) = &self.user {
            text = replace(&text, user, "<user>", |b| !is_word(b) && b != b'.');
        }
        for host in &self.hosts {
            text = replace(&text, host, "<host>", |b| !is_word(b) && b != b'.');
        }
        if long { long_strings(&text) } else { text }
    }
}

/// `text` with each `needle` that stands alone replaced: the bytes around it
/// satisfy `apart`.
fn replace(text: &str, needle: &str, with: &str, apart: fn(u8) -> bool) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for (at, _) in text.match_indices(needle) {
        let end = at + needle.len();
        let before = at.checked_sub(1).is_none_or(|b| apart(bytes[b]));
        let after = bytes.get(end).is_none_or(|b| apart(*b));
        if before && after {
            out.push_str(&text[copied..at]);
            out.push_str(with);
            copied = end;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// `/var/folders/xx/yyyy/T` (also under `/private`) becomes `<tmpdir>`.
fn temp_dirs(text: &str) -> String {
    const ROOT: &str = "/var/folders/";
    let stop = |c: char| c == '/' || c.is_whitespace() || "\"'`:,;)]}".contains(c);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(ROOT) {
        let start = rest[..at].strip_suffix("/private").map_or(at, str::len);
        let mut end = at + ROOT.len();
        for part in 0..3 {
            if part > 0 {
                if !rest[end..].starts_with('/') {
                    break;
                }
                end += 1;
            }
            end += rest[end..].find(stop).unwrap_or(rest.len() - end);
        }
        out.push_str(&rest[..start]);
        out.push_str("<tmpdir>");
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// Each run of base64 or hex characters of [`LONG`] or more that mixes
/// letters and digits, and is not a path, becomes `<redacted>`.
fn long_strings(text: &str) -> String {
    let token = |b: u8| b.is_ascii_alphanumeric() || b"+/=_-".contains(&b);
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        let len = bytes[at..].iter().take_while(|b| token(**b)).count();
        if len == 0 {
            let next = text[at..].chars().next().map_or(1, char::len_utf8);
            out.push_str(&text[at..at + next]);
            at += next;
            continue;
        }
        let run = &bytes[at..at + len];
        let slashes = run.iter().filter(|b| **b == b'/').count();
        let secret = len >= LONG
            && run.iter().any(u8::is_ascii_alphabetic)
            && run.iter().any(u8::is_ascii_digit)
            && slashes * 16 <= len;
        out.push_str(if secret {
            REDACTED
        } else {
            &text[at..at + len]
        });
        at += len;
    }
    out
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    /// A made-up machine: user `jdoe-example` on host `example-mbp.local`.
    fn redactor() -> Redactor {
        Redactor::new(Some("/Users/jdoe-example"), Some("example-mbp.local"))
    }

    const HASH: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn unit_redaction_full_tier_covers_every_rule_of_the_table() {
        let full = redactor();
        for (input, expected) in [
            // Paths: the home directory, the user name, the temporary directory.
            ("open /Users/jdoe-example/proj/a.rs", "open ~/proj/a.rs"),
            (
                "owner jdoe-example, not jdoe-example2",
                "owner <user>, not jdoe-example2",
            ),
            ("/Users/jdoe-example2/x", "/Users/jdoe-example2/x"),
            (
                "at /var/folders/xy/ab12cd34/T/cupper-1/sock",
                "at <tmpdir>/cupper-1/sock",
            ),
            ("\"/private/var/folders/xy/ab12cd34/T\"", "\"<tmpdir>\""),
            // The host name, with and without its domain.
            (
                "on example-mbp.local and example-mbp",
                "on <host> and <host>",
            ),
            // Credentials, as in the structured tier.
            (
                "Authorization: Bearer abc.def-123_456",
                "Authorization: Bearer <redacted>",
            ),
            ("key sk-ant-api03-AbCdEf0123456789xyz", "key sk-<redacted>"),
            // Long strings that mix letters and digits: hex and base64.
            (&format!("sha {HASH}"), "sha <redacted>"),
            (
                "data iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4",
                "data <redacted>",
            ),
            // Paths and words stay, however long.
            (
                "/opt/homebrew/Cellar/node/24.1.0/lib/node_modules/abc123",
                "/opt/homebrew/Cellar/node/24.1.0/lib/node_modules/abc123",
            ),
            (
                "cupper_host::orchestrator::session::actor",
                "cupper_host::orchestrator::session::actor",
            ),
        ] {
            assert_eq!(full.text(input), expected, "{input}");
        }
    }

    #[test]
    fn unit_redaction_the_hosts_own_hashes_and_ids_pass_by_field_name() {
        let full = redactor();
        let id = "s_0199a1b2c3d47e8f9a0b1c2d3e4f5a6b7c8d9e0f1a2b3c4d";
        for field in ["id", "session_id", "blob_hash", "digest", "blob"] {
            assert_eq!(full.field(field, HASH), HASH, "{field}");
        }
        assert_eq!(full.field("session_id", id), id);
        assert_eq!(full.field("message", HASH), REDACTED);
        // Only the long-string rule is waived.
        let path = "/Users/jdoe-example/x";
        assert_eq!(full.field("blob_hash", path), "~/x");
    }

    #[test]
    fn unit_redaction_structured_tier_touches_credentials_only() {
        let structured = |text| credentials(text);
        let json =
            format!(r#"{{"hash":"{HASH}","data":"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJ"}}"#);
        for text in [
            "open /Users/jdoe-example/proj/a.rs on example-mbp.local",
            "/var/folders/xy/ab12cd34/T/cupper-1",
            &json,
        ] {
            assert!(matches!(structured(text), Cow::Borrowed(_)), "{text}");
        }
        assert_eq!(structured("Bearer abc.def-123_456"), "Bearer <redacted>");
    }
}
