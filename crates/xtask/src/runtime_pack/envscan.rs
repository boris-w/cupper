//! The environment variables an adapter reads (design 5.6.6). The scan is
//! static and leans towards finding too much: it takes the member reads of
//! every identifier `env` or `...Env` (`process.env`, `process2.env`,
//! `options.env`, `spawnEnv`): `.NAME`, `?.NAME` and `["NAME"]`, also after a
//! closing parenthesis as in `(a ?? process.env).NAME`. Reads with a computed
//! key are only counted. Not seen: minified aliases (`let e = process.env;
//! e.NAME`), `"NAME" in env` and destructuring (neither occurs in the pinned
//! adapters), and names looked up through a table, such as the SDK's own
//! registry of variables. Every name found must fall into one of the
//! manifest's classes, or `runtime-pack` fails and a person classifies it.

use std::collections::BTreeSet;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

use super::manifest::Env;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reads {
    pub names: BTreeSet<String>,
    /// Reads with a computed key, `env[name]`.
    pub dynamic: u32,
}

pub fn scan(source: &str) -> Reads {
    let bytes = source.as_bytes();
    let mut reads = Reads::default();
    let mut at = 0;
    while let Some((start, end)) = next_identifier(bytes, at) {
        at = end;
        let identifier = &source[start..end];
        if identifier != "env" && !identifier.ends_with("Env") {
            continue;
        }
        let mut next = skip_space(bytes, end);
        if bytes.get(next) == Some(&b')') {
            next = skip_space(bytes, next + 1);
        }
        match member(source, next) {
            // Other objects called `env` hold anything; from them only names
            // that look like variables count (`env.NAME`, not `env.name`).
            Some(Some(name))
                if on_process(bytes, start)
                    || name.contains('_')
                    || name.starts_with(|c: char| c.is_ascii_uppercase()) =>
            {
                reads.names.insert(name);
            }
            Some(None) => reads.dynamic += 1,
            _ => {}
        }
    }
    reads
}

/// Fails when an adapter reads a name that no class covers. A class entry is
/// a name or a prefix ending in `*`.
pub fn check(env: &Env) -> Result<()> {
    let classes = [&env.allow, &env.deny, &env.harmless];
    let known = |name: &str| {
        classes.iter().flat_map(|class| class.iter()).any(|entry| {
            entry
                .strip_suffix('*')
                .map_or(entry == name, |prefix| name.starts_with(prefix))
        })
    };
    let unclassified: Vec<String> = env
        .reads
        .iter()
        .flat_map(|(adapter, reads)| {
            let names = reads.names.iter().filter(|name| !known(name));
            names.map(move |name| format!("{name} ({adapter})"))
        })
        .collect();
    if !unclassified.is_empty() {
        bail!(
            "adapters read environment variables that are not classified; add each to `allow`, \
             `deny` or `harmless` in runtime/manifest.json (design 5.6.6):\n  {}",
            unclassified.join("\n  ")
        );
    }
    Ok(())
}

/// A member read starting at `at`: `Some(Some(name))` for `.NAME`, `?.NAME`
/// (not a method call) or `["NAME"]`; `Some(None)` for a computed `[key]`.
fn member(source: &str, at: usize) -> Option<Option<String>> {
    let bytes = source.as_bytes();
    let rest = &bytes[at.min(bytes.len())..];
    let (open, bracket) = if rest.starts_with(b"?.[") {
        (at + 3, true)
    } else if rest.starts_with(b"?.") {
        (at + 2, false)
    } else if rest.starts_with(b"[") {
        (at + 1, true)
    } else if rest.starts_with(b".") && !rest.starts_with(b"..") {
        (at + 1, false)
    } else {
        return None;
    };
    if bracket {
        let quote = skip_space(bytes, open);
        let name = bytes
            .get(quote)
            .filter(|byte| matches!(byte, b'"' | b'\'' | b'`'))
            .and_then(|byte| {
                let close = quote + 1 + bytes[quote + 1..].iter().position(|b| b == byte)?;
                let after = skip_space(bytes, close + 1);
                (bytes.get(after) == Some(&b']')).then(|| valid_name(&source[quote + 1..close]))?
            });
        return Some(name);
    }
    let (start, end) = next_identifier(bytes, open).filter(|(start, _)| *start == open)?;
    let called = bytes.get(skip_space(bytes, end)) == Some(&b'(');
    (!called)
        .then(|| valid_name(&source[start..end]))
        .flatten()
        .map(Some)
}

/// Whether the identifier at `start` follows `process.` or `process2.`
/// (esbuild renames the imports of `node:process`).
fn on_process(bytes: &[u8], start: usize) -> bool {
    let Some(dot) = start.checked_sub(1).filter(|dot| bytes[*dot] == b'.') else {
        return false;
    };
    let digits = bytes[..dot]
        .iter()
        .rev()
        .take_while(|b| b.is_ascii_digit())
        .count();
    let object = &bytes[..dot - digits];
    object.ends_with(b"process")
        && !object[..object.len() - 7]
            .last()
            .is_some_and(|b| is_identifier(*b))
}

fn valid_name(text: &str) -> Option<String> {
    let mut chars = text.chars();
    let first = chars.next()?;
    let valid = (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| text.to_owned())
}

/// The next identifier at or after `at` that does not continue a longer one.
fn next_identifier(bytes: &[u8], at: usize) -> Option<(usize, usize)> {
    let start = (at..bytes.len()).find(|&index| {
        let begins = is_identifier(bytes[index]) && !bytes[index].is_ascii_digit();
        begins && (index == 0 || !is_identifier(bytes[index - 1]))
    })?;
    let len = bytes[start..]
        .iter()
        .take_while(|b| is_identifier(**b))
        .count();
    Some((start, start + len))
}

fn is_identifier(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'$'
}

fn skip_space(bytes: &[u8], at: usize) -> usize {
    let rest = &bytes[at.min(bytes.len())..];
    at + rest.iter().take_while(|b| b.is_ascii_whitespace()).count()
}

#[cfg(test)]
pub mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn names(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    pub fn env(reads: &[&str]) -> Env {
        let class = |list: &[&str]| list.iter().map(|name| (*name).to_owned()).collect();
        Env {
            allow: class(&["PATH", "LC_*"]),
            deny: class(&["CODEX_PATH", "NODE_*"]),
            harmless: class(&["TERM_PROGRAM"]),
            reads: BTreeMap::from([(
                "codex_acp".to_owned(),
                Reads {
                    names: names(reads),
                    dynamic: 0,
                },
            )]),
        }
    }

    #[test]
    fn unit_runtime_pack_envscan_finds_member_reads_on_env_objects() {
        let source = r#"
            const a = process.env.ALPHA, b = process2.env["BETA"], c = process.env?.GAMMA;
            if (process.env['DELTA'] === "1" || (opts.env ?? process.env).EPSILON) {}
            const d = process.env?.["ZETA"] ?? spawnEnv.ETA ?? env.http_proxy;
            process.env.comspec || process.env[ key ] || env[`THETA`];
            env.hasOwnProperty("X"); env.name; schemaEnv.root; process.env.$weird;
            reprocess.env.lower; const environment = 1; "process.env.IOTA";
        "#;
        let reads = scan(source);
        let want = [
            "ALPHA",
            "BETA",
            "DELTA",
            "EPSILON",
            "ETA",
            "GAMMA",
            "IOTA",
            "THETA",
            "ZETA",
            "comspec",
            "http_proxy",
        ];
        assert_eq!(reads.names, names(&want));
        assert_eq!(reads.dynamic, 1, "process.env[ key ]");
    }

    #[test]
    fn unit_runtime_pack_envscan_an_unclassified_name_fails_the_command() {
        check(&env(&[
            "PATH",
            "LC_ALL",
            "NODE_OPTIONS",
            "CODEX_PATH",
            "TERM_PROGRAM",
        ]))
        .expect("every name is classified, by name or prefix");

        let err = check(&env(&["PATH", "CODEX_NEW_SWITCH", "NODE"])).expect_err("new names");
        let text = err.to_string();
        assert!(text.contains("CODEX_NEW_SWITCH (codex_acp)"), "{text}");
        assert!(
            text.contains("NODE (codex_acp)"),
            "`NODE_*` does not cover NODE: {text}"
        );
        assert!(!text.contains("PATH (codex_acp)"), "{text}");
    }
}
