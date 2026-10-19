//! The environment of a child process (design 5.8.5).
//!
//! A child never inherits cupper's own environment: it gets exactly the
//! variables of its [`Env`], which the caller fills from an allow-list. Which
//! names go on the list is the caller's policy (design 5.6.6 for adapters).

use std::collections::BTreeMap;
use std::ffi::{CString, OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

use crate::{Error, Result};

/// No child of cupper receives a variable with this prefix, whatever the
/// allow-list says (ADR-05).
const DENIED_PREFIX: &[u8] = b"CUPPER_";

/// The complete environment of a child.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    vars: BTreeMap<OsString, OsString>,
}

impl Env {
    /// An empty environment.
    pub fn new() -> Self {
        Self::default()
    }

    /// The variables of `source` whose names match a pattern of `allow`. A
    /// pattern ending in `*` matches names with that prefix (`LC_*`); any other
    /// pattern matches one name. Entries that [`Env::set`] would refuse are
    /// skipped.
    pub fn from_allowed<I, K, V, P>(source: I, allow: &[P]) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<OsString>,
        V: Into<OsString>,
        P: AsRef<str>,
    {
        let mut env = Self::new();
        for (name, value) in source {
            let name = name.into();
            if allow.iter().any(|p| matches(p.as_ref(), &name)) {
                // A refused entry is skipped on purpose: see the doc comment.
                let _ = env.set(name, value);
            }
        }
        env
    }

    /// Sets `name` to `value`, replacing an earlier value. Refuses names that
    /// are empty or contain `=` or NUL, values that contain NUL, and names
    /// starting with `CUPPER_`.
    pub fn set(&mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Result<()> {
        let name = name.into();
        let value = value.into();
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.contains(&b'=') || bytes.contains(&0) {
            return Err(Error::InvalidArgument("environment variable name"));
        }
        if bytes.starts_with(DENIED_PREFIX) {
            return Err(Error::InvalidArgument(
                "CUPPER_* variables are never passed to a child",
            ));
        }
        if value.as_bytes().contains(&0) {
            return Err(Error::InvalidArgument("NUL in environment variable value"));
        }
        self.vars.insert(name, value);
        Ok(())
    }

    pub fn get(&self, name: impl AsRef<OsStr>) -> Option<&OsStr> {
        self.vars.get(name.as_ref()).map(OsString::as_os_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&OsStr, &OsStr)> {
        self.vars
            .iter()
            .map(|(k, v)| (k.as_os_str(), v.as_os_str()))
    }

    /// `NAME=value` strings for `posix_spawn`.
    pub(crate) fn to_cstrings(&self) -> Vec<CString> {
        self.vars
            .iter()
            .map(|(name, value)| {
                let mut entry = Vec::with_capacity(name.len() + 1 + value.len());
                entry.extend_from_slice(name.as_bytes());
                entry.push(b'=');
                entry.extend_from_slice(value.as_bytes());
                CString::new(entry).expect("`set` refuses NUL bytes")
            })
            .collect()
    }
}

fn matches(pattern: &str, name: &OsStr) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.as_bytes().starts_with(prefix.as_bytes()),
        None => name.as_bytes() == pattern.as_bytes(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_sys_spawn_env_takes_only_allowed_names() {
        let source = [
            ("PATH", "/usr/bin:/bin"),
            ("LC_ALL", "C"),
            ("LC_CTYPE", "UTF-8"),
            ("LCX", "not a locale variable"),
            ("PATHX", "not PATH"),
            ("SECRET_TOKEN", "x"),
            ("CUPPER_DEV_KEY", "never passed"),
        ];
        let env = Env::from_allowed(source, &["PATH", "LC_*", "CUPPER_*"]);
        let got: Vec<_> = env.iter().collect();
        let want: Vec<(&OsStr, &OsStr)> = [
            ("LC_ALL", "C"),
            ("LC_CTYPE", "UTF-8"),
            ("PATH", "/usr/bin:/bin"),
        ]
        .iter()
        .map(|(k, v)| (OsStr::new(k), OsStr::new(v)))
        .collect();
        assert_eq!(got, want);
    }

    #[test]
    fn unit_sys_spawn_env_refuses_bad_and_cupper_names() {
        let mut env = Env::new();
        assert!(env.set("CUPPER_DEV_KEY", "x").is_err());
        assert!(env.set("", "x").is_err());
        assert!(env.set("A=B", "x").is_err());
        assert!(env.set("A\0B", "x").is_err());
        assert!(env.set("A", "x\0y").is_err());
        assert_eq!(env, Env::new());
        env.set("LANG", "C").unwrap();
        env.set("LANG", "en_US.UTF-8").unwrap();
        assert_eq!(env.get("LANG"), Some(OsStr::new("en_US.UTF-8")));
        assert_eq!(env.to_cstrings(), [c"LANG=en_US.UTF-8".to_owned()]);
    }
}
