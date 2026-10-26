//! The snapshot tier's `config` (design 5.2.10, "配置白名单"): only the keys
//! of the user's effective configuration that decide how the work tree
//! turns into objects, none of which runs a program. The one exception is
//! git-lfs: its standard filters, with the program the host resolved at
//! start. Pure.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::super::rules::{Entry, lfs_arguments, quote, truthy};

/// The whitelist, as git prints the keys.
const KEYS: &[&str] = &[
    "core.autocrlf",
    "core.eol",
    "core.ignorecase",
    "core.precomposeunicode",
    "core.filemode",
    "core.symlinks",
    "core.checkstat",
    "core.trustctime",
    "core.bigfilethreshold",
    "core.protecthfs",
    "core.protectntfs",
    "core.excludesfile",
    "core.attributesfile",
    "core.untrackedcache",
    "core.splitindex",
    "core.sparsecheckout",
    "core.sparsecheckoutcone",
    "index.sparse",
    "index.version",
    "index.skiphash",
    "feature.manyfiles",
    "extensions.objectformat",
];

const LFS: [&str; 3] = [
    "filter.lfs.clean",
    "filter.lfs.smudge",
    "filter.lfs.process",
];

/// The value of each key in effect: the last one.
fn last(entries: &[Entry]) -> BTreeMap<&[u8], Option<&[u8]>> {
    (entries.iter())
        .map(|entry| (entry.key.as_slice(), entry.value.as_deref()))
        .collect()
}

/// The host runs git-lfs's filters: they are its standard values, and the
/// host has git-lfs.
fn runs_lfs(last: &BTreeMap<&[u8], Option<&[u8]>>, lfs: Option<&Path>) -> bool {
    let standard = |key: &str| {
        let value = last.get(key.as_bytes()).copied().flatten();
        lfs_arguments(key.as_bytes(), value).is_some()
    };
    lfs.is_some() && LFS.iter().all(|key| standard(key))
}

/// The `config` file for `entries`, the user's effective configuration.
pub(in crate::git) fn text(entries: &[Entry], lfs: Option<&Path>) -> Vec<u8> {
    let last = last(entries);
    let mut out = b"[core]\n\tbare = false\n".to_vec();
    let mut set = |section: &str, name: &str, value: &[u8]| {
        out.extend(format!("[{section}]\n\t{name} = \"").bytes());
        for byte in value {
            match byte {
                b'"' | b'\\' => out.extend([b'\\', *byte]),
                b'\n' => out.extend(b"\\n"),
                _ => out.push(*byte),
            }
        }
        out.extend(b"\"\n");
    };
    for key in KEYS {
        let Some(&value) = last.get(key.as_bytes()) else {
            continue;
        };
        let (section, name) = key.split_once('.').expect("section.name");
        set(section, name, value.unwrap_or(b"true"));
        if *key == "extensions.objectformat" && value.is_some_and(|v| v != b"sha1") {
            set("core", "repositoryformatversion", b"1");
        }
    }
    if let Some(lfs) = lfs.filter(|_| runs_lfs(&last, lfs)) {
        for key in LFS {
            let value = last.get(key.as_bytes()).copied().flatten();
            let arguments = lfs_arguments(key.as_bytes(), value).expect("standard");
            set(
                "filter \"lfs\"",
                &key[11..],
                &[quote(lfs), arguments.to_vec()].concat(),
            );
        }
        if last
            .get(b"filter.lfs.required".as_slice())
            .is_some_and(|v| truthy(*v))
        {
            set("filter \"lfs\"", "required", b"true");
        }
    }
    out
}

/// The filters the configuration defines and the snapshot tier does not
/// run: every one with a `clean` or `process` program, git-lfs's too unless
/// the host runs it.
pub(in crate::git) fn unrun(entries: &[Entry], lfs: Option<&Path>) -> BTreeSet<Vec<u8>> {
    let last = last(entries);
    let runs_lfs = runs_lfs(&last, lfs);
    let defined = last
        .iter()
        .filter(|(_, value)| value.is_some_and(|v| !v.is_empty()));
    let names = defined.filter_map(|(key, _)| {
        let rest = key.strip_prefix(b"filter.")?;
        let name = (rest.strip_suffix(b".clean")).or_else(|| rest.strip_suffix(b".process"))?;
        Some(name.to_vec())
    });
    names.filter(|name| !(runs_lfs && name == b"lfs")).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::git::rules::Origin;

    fn entries(pairs: &[(&str, Option<&str>)]) -> Vec<Entry> {
        let entry = |(key, value): &(&str, Option<&str>)| Entry {
            origin: Origin::File(PathBuf::from("/repo/.git/config")),
            key: key.as_bytes().to_vec(),
            value: value.map(|v| v.as_bytes().to_vec()),
        };
        pairs.iter().map(entry).collect()
    }

    #[test]
    fn unit_git_snapshot_config_keeps_the_whitelist_and_git_lfs_only() {
        let lfs_values = [
            ("filter.lfs.clean", Some("git-lfs clean -- %f")),
            ("filter.lfs.smudge", Some("git-lfs smudge -- %f")),
            ("filter.lfs.process", Some("git-lfs filter-process")),
            ("filter.lfs.required", Some("true")),
        ];
        let mut user = vec![
            ("core.autocrlf", Some("false")),
            ("core.autocrlf", Some("true")),
            ("core.excludesfile", Some("~/a \"b\"\\c")),
            ("core.fsmonitor", Some("/evil")),
            ("core.ignorecase", None),
            ("extensions.objectformat", Some("sha256")),
            ("filter.crypt.clean", Some("crypt clean")),
            ("diff.x.textconv", Some("/evil")),
            ("user.name", Some("someone")),
        ];
        user.extend(lfs_values);
        let lfs = Path::new("/opt/git lfs/git-lfs");
        let written = String::from_utf8(text(&entries(&user), Some(lfs))).unwrap();
        let expected = "[core]\n\tbare = false\n[core]\n\tautocrlf = \"true\"\n\
            [core]\n\tignorecase = \"true\"\n[core]\n\texcludesfile = \"~/a \\\"b\\\"\\\\c\"\n\
            [extensions]\n\tobjectformat = \"sha256\"\n[core]\n\trepositoryformatversion = \"1\"\n\
            [filter \"lfs\"]\n\tclean = \"'/opt/git lfs/git-lfs' clean -- %f\"\n\
            [filter \"lfs\"]\n\tsmudge = \"'/opt/git lfs/git-lfs' smudge -- %f\"\n\
            [filter \"lfs\"]\n\tprocess = \"'/opt/git lfs/git-lfs' filter-process\"\n\
            [filter \"lfs\"]\n\trequired = \"true\"\n";
        assert_eq!(written, expected);
        let unrun = |lfs| unrun(&entries(&user), lfs).into_iter().collect::<Vec<_>>();
        assert_eq!(unrun(Some(lfs)), [b"crypt".to_vec()]);
        // Without the host's git-lfs, or with other values, LFS stays out.
        assert_eq!(unrun(None), [b"crypt".to_vec(), b"lfs".to_vec()]);
        user[9].1 = Some("/elsewhere/git-lfs clean -- %f");
        assert!(
            !String::from_utf8(text(&entries(&user), Some(lfs)))
                .unwrap()
                .contains("lfs")
        );
    }
}
