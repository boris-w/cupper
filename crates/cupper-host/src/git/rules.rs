//! The repo tier's rules (design 5.2.10, "仓库档：每次调用前读配置" 1–5): what
//! `git config --list --show-origin --includes -z` says, whether its sources
//! can be trusted, and what every call in the user's repository adds so that
//! no program its configuration names runs. Pure: the caller runs git and
//! hands over the bytes; only the realpaths read the file system.
//!
//! Every [`Unsafe`] is `git_error` with `reason: unsafe_repo_config`.

use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// One entry of the effective configuration, as bytes: git writes what the
/// files hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub origin: Origin,
    /// `section.subsection.name`; git lowercases the section and the name.
    pub key: Vec<u8>,
    /// `None` for a bare key (`[core] flag`), which reads as true.
    pub value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A file; a relative path is joined to the directory git ran in.
    File(PathBuf),
    /// `-c` and `GIT_CONFIG_PARAMETERS`: the host's own.
    CommandLine,
    /// `blob:`, `standard input:` or any other.
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Unsafe {
    #[error("the configuration output is malformed")]
    Malformed,
    #[error("configuration comes from `{0}`, which is not a file")]
    Origin(String),
    #[error("configuration file {0} cannot be resolved")]
    Unresolved(PathBuf),
    #[error("configuration file {0} lies where an agent can write")]
    Writable(PathBuf),
    /// `-c` splits at the first `=`, so such a key cannot be emptied.
    #[error("{0} names a program and has `=` in its subsection")]
    Unnullable(String),
}

/// The entries of `output`, which git printed while running in `cwd`: each
/// is `<origin>\0<key>\n<value>\0`, or `<origin>\0<key>\0` without a value.
pub fn parse(output: &[u8], cwd: &Path) -> Result<Vec<Entry>, Unsafe> {
    if output.is_empty() {
        return Ok(Vec::new());
    }
    let body = output.strip_suffix(b"\0").ok_or(Unsafe::Malformed)?;
    let fields: Vec<&[u8]> = body.split(|b| *b == 0).collect();
    if !fields.len().is_multiple_of(2) {
        return Err(Unsafe::Malformed);
    }
    let entry = |pair: &[&[u8]]| {
        let origin = match pair[0].strip_prefix(b"file:") {
            Some(path) => Origin::File(cwd.join(OsStr::from_bytes(path))),
            None if pair[0] == b"command line:" => Origin::CommandLine,
            None => Origin::Other(String::from_utf8_lossy(pair[0]).into_owned()),
        };
        let (key, value) = match pair[1].iter().position(|b| *b == b'\n') {
            Some(at) => (&pair[1][..at], Some(pair[1][at + 1..].to_vec())),
            None => (pair[1], None),
        };
        let key = key.to_vec();
        Entry { origin, key, value }
    };
    Ok(fields.chunks(2).map(entry).collect())
}

/// Where configuration may come from.
#[derive(Debug, Clone, Copy)]
pub struct Bounds<'a> {
    /// The work tree's git directory; its `config.worktree` is the repo's own.
    pub git_dir: &'a Path,
    /// The common dir; its `config` is the repo's own.
    pub common_dir: &'a Path,
    /// What agents can write: every project and every session's workspace.
    pub writable: &'a [PathBuf],
}

/// Refuses configuration from a file where an agent can write, except the
/// repository's own `config` and `config.worktree`, which the overrides and
/// the fingerprint cover (rule 2), and from anything but a file or the
/// command line. Paths are compared as realpaths.
pub fn check_sources(entries: &[Entry], bounds: &Bounds<'_>) -> Result<(), Unsafe> {
    let real = |path: &Path| path.canonicalize().ok();
    let own = [
        bounds.common_dir.join("config"),
        bounds.git_dir.join("config.worktree"),
    ];
    let own: Vec<PathBuf> = own.iter().filter_map(|path| real(path)).collect();
    // A root that is gone holds no configuration.
    let roots: Vec<PathBuf> = bounds.writable.iter().filter_map(|r| real(r)).collect();
    let mut checked = BTreeSet::new();
    for entry in entries {
        match &entry.origin {
            Origin::CommandLine => {}
            Origin::Other(name) => return Err(Unsafe::Origin(name.clone())),
            Origin::File(path) if checked.insert(path) => {
                let file = real(path).ok_or_else(|| Unsafe::Unresolved(path.clone()))?;
                if !own.contains(&file) && roots.iter().any(|root| file.starts_with(root)) {
                    return Err(Unsafe::Writable(file));
                }
            }
            Origin::File(_) => {}
        }
    }
    Ok(())
}

/// What a call needs to know to override the configuration.
#[derive(Debug, Clone, Copy)]
pub struct Setup<'a> {
    /// `<data dir>/git/empty-hooks`.
    pub hooks: &'a Path,
    /// The git-lfs the host resolved at start, if any.
    pub lfs: Option<&'a Path>,
    /// The command makes patches.
    pub patch: bool,
}

/// What every call in the user's repository adds (rules 3–5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Overrides {
    /// Before the subcommand: `--no-pager` and `-c` pairs.
    pub global: Vec<OsString>,
    /// After it: `--no-ext-diff --no-textconv` for a command that makes
    /// patches.
    pub command: Vec<OsString>,
    /// Filters with `required=true` that lost a program: if the call fails,
    /// it is `unsafe_repo_config` (rule 5).
    pub required: Vec<String>,
}

/// The overrides for `entries`: the hooks and fsmonitor off, and every key
/// that names a program emptied, except git-lfs's standard values, whose
/// program becomes the host's git-lfs.
pub fn overrides(entries: &[Entry], setup: &Setup<'_>) -> Result<Overrides, Unsafe> {
    // The last value of a key is the one in effect.
    let last: BTreeMap<&[u8], Option<&[u8]>> = (entries.iter())
        .map(|entry| (entry.key.as_slice(), entry.value.as_deref()))
        .collect();
    let mut global = vec![OsString::from("--no-pager")];
    let mut set = |key: &[u8], value: &[u8]| {
        global.push("-c".into());
        global.push(OsString::from_vec([key, b"=", value].concat()));
    };
    set(b"core.fsmonitor", b"false");
    set(b"core.hooksPath", setup.hooks.as_os_str().as_bytes());
    let mut emptied = BTreeSet::new();
    for (&key, &value) in &last {
        let Some((section, sub, name)) = split(key) else {
            continue;
        };
        if !names_program(section, sub, name) {
            continue;
        }
        if sub.is_some_and(|sub| sub.contains(&b'=')) {
            return Err(Unsafe::Unnullable(
                String::from_utf8_lossy(key).into_owned(),
            ));
        }
        match (setup.lfs, lfs_arguments(key, value)) {
            (Some(lfs), Some(arguments)) => set(key, &[quote(lfs), arguments.to_vec()].concat()),
            _ => {
                set(key, b"");
                if section == b"filter" {
                    emptied.extend(sub);
                }
            }
        }
    }
    let required = (emptied.into_iter())
        .filter(|filter| {
            let key = [b"filter.", *filter, b".required"].concat();
            last.get(key.as_slice()).is_some_and(|value| truthy(*value))
        })
        .map(|filter| String::from_utf8_lossy(filter).into_owned())
        .collect();
    let command = match setup.patch {
        true => vec!["--no-ext-diff".into(), "--no-textconv".into()],
        false => Vec::new(),
    };
    Ok(Overrides {
        global,
        command,
        required,
    })
}

/// A key's section, subsection and name.
type Parts<'a> = (&'a [u8], Option<&'a [u8]>, &'a [u8]);

/// The parts of `key`: the subsection is what lies between the first and the
/// last dot, and may hold dots itself.
fn split(key: &[u8]) -> Option<Parts<'_>> {
    let first = key.iter().position(|b| *b == b'.')?;
    let last = key.iter().rposition(|b| *b == b'.')?;
    let sub = (first < last).then(|| &key[first + 1..last]);
    Some((&key[..first], sub, &key[last + 1..]))
}

/// The keys that name a program (rule 4, with the five T060 added).
fn names_program(section: &[u8], sub: Option<&[u8]>, name: &[u8]) -> bool {
    matches!(
        (section, sub.is_some(), name),
        (b"filter", true, b"clean" | b"smudge" | b"process")
            | (b"diff", true, b"command" | b"textconv")
            | (b"diff", false, b"external")
            | (b"merge", true, b"driver")
            | (
                b"core",
                false,
                b"sshcommand" | b"pager" | b"editor" | b"askpass"
            )
            | (b"core", false, b"gitproxy" | b"alternaterefscommand")
            | (b"sequence", false, b"editor")
            | (b"gpg", _, b"program")
            | (b"gpg", true, b"defaultkeycommand")
            | (b"credential", _, b"helper")
            | (b"uploadpack", false, b"packobjectshook")
            | (b"remote", true, b"receivepack" | b"uploadpack")
            | (b"submodule", true, b"update")
            | (b"pager", false, _)
    )
}

/// The arguments after the program in git-lfs's standard value for `key`,
/// if `value` is that.
fn lfs_arguments(key: &[u8], value: Option<&[u8]>) -> Option<&'static [u8]> {
    match (key, value?) {
        (b"filter.lfs.clean", b"git-lfs clean -- %f") => Some(b" clean -- %f"),
        (b"filter.lfs.smudge", b"git-lfs smudge -- %f") => Some(b" smudge -- %f"),
        (b"filter.lfs.process", b"git-lfs filter-process") => Some(b" filter-process"),
        _ => None,
    }
}

/// `path` single-quoted for the shell git runs filters with.
fn quote(path: &Path) -> Vec<u8> {
    let parts: Vec<&[u8]> = path.as_os_str().as_bytes().split(|b| *b == b'\'').collect();
    [&b"'"[..], &parts.join(&b"'\\''"[..]), b"'"].concat()
}

/// git's boolean: a bare key, `true`, `yes`, `on` or a non-zero integer.
fn truthy(value: Option<&[u8]>) -> bool {
    let Some(value) = value else {
        return true;
    };
    let value = String::from_utf8_lossy(value).to_ascii_lowercase();
    matches!(value.as_str(), "true" | "yes" | "on") || value.parse::<i64>().is_ok_and(|n| n != 0)
}

#[cfg(test)]
mod tests;
