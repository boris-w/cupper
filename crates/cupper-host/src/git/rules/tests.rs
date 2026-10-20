//! UNIT-git-repo-rules: the configuration output, its sources and the
//! overrides, on layouts made in a temporary directory.

use std::os::unix::fs::symlink;

use super::*;
use crate::store::testutil::TempDir;

fn entry(origin: Origin, key: &str, value: Option<&str>) -> Entry {
    let value = value.map(|v| v.as_bytes().to_vec());
    let key = key.as_bytes().to_vec();
    Entry { origin, key, value }
}

fn file(path: &str) -> Origin {
    Origin::File(path.into())
}

fn setup(lfs: Option<&Path>, patch: bool) -> Setup<'_> {
    let hooks = Path::new("/data/git/empty-hooks");
    Setup { hooks, lfs, patch }
}

/// The `-c` values of `overrides`, past the unconditional ones.
fn emptied(entries: &[Entry], lfs: Option<&Path>) -> Result<Vec<String>, Unsafe> {
    let overrides = overrides(entries, &setup(lfs, false))?;
    let args = overrides.global[5..].iter().skip(1).step_by(2);
    Ok(args.map(|arg| arg.to_string_lossy().into_owned()).collect())
}

#[test]
fn unit_git_repo_rules_parse_keeps_bytes_and_joins_relative_paths() {
    let output = [
        &b"file:.git/config\0core.bare\nfalse\0"[..],
        b"file:.git/../inc\0x.y\nline 1\nline 2=3\0",
        b"file:.git/config\0core.flag\0",
        b"file:/etc/gitconfig\0sub.\xff.key\nv\xfe\0",
        b"command line:\0a.b\nc\0",
        b"blob:HEAD:.gitmodules\0submodule.s.url\nu\0",
    ];
    let cwd = Path::new("/repo");
    let entries = parse(&output.concat(), cwd).unwrap();
    let mut odd = entry(file("/etc/gitconfig"), "", None);
    (odd.key, odd.value) = (b"sub.\xff.key".to_vec(), Some(b"v\xfe".to_vec()));
    let blob = Origin::Other("blob:HEAD:.gitmodules".into());
    let expected = [
        entry(file("/repo/.git/config"), "core.bare", Some("false")),
        entry(file("/repo/.git/../inc"), "x.y", Some("line 1\nline 2=3")),
        entry(file("/repo/.git/config"), "core.flag", None),
        odd,
        entry(Origin::CommandLine, "a.b", Some("c")),
        entry(blob, "submodule.s.url", Some("u")),
    ];
    assert_eq!(entries, expected);
    assert_eq!(parse(b"", cwd), Ok(Vec::new()));
    for bad in [&b"file:a\0k\nv"[..], b"file:a\0k\nv\0file:b\0"] {
        assert_eq!(parse(bad, cwd), Err(Unsafe::Malformed));
    }
}

#[test]
fn unit_git_repo_rules_sources_outside_agents_reach_or_the_repos_own() {
    let dir = TempDir::new("git-sources");
    let at = |path: &str| dir.path().join(path);
    let files = "p/.git/config p/.git/config.worktree p/.git/inc p/x.inc q/y.inc \
        p/.git/worktrees/w/config.worktree home/.gitconfig";
    for file in files.split_whitespace() {
        std::fs::create_dir_all(at(file).parent().unwrap()).unwrap();
        std::fs::write(at(file), "").unwrap();
    }
    symlink(at("p/x.inc"), at("home/link.inc")).unwrap();
    // A linked worktree of the project p; p and q are projects.
    let (git_dir, common_dir) = (at("p/.git/worktrees/w"), at("p/.git"));
    let writable = [at("p"), at("q"), at("gone")];
    let bounds = Bounds {
        git_dir: &git_dir,
        common_dir: &common_dir,
        writable: &writable,
    };
    let check = |origin| check_sources(&[entry(origin, "x.y", Some("1"))], &bounds);
    // The origin, and the realpath refused, if it is.
    for (origin, refused) in [
        ("p/.git/config", None),
        ("p/.git/worktrees/w/config.worktree", None),
        ("home/.gitconfig", None),
        ("p/.git/config.worktree", Some("p/.git/config.worktree")),
        ("p/.git/inc", Some("p/.git/inc")),
        ("p/x.inc", Some("p/x.inc")),
        ("q/y.inc", Some("q/y.inc")),
        // Outside by name, inside by realpath.
        ("home/link.inc", Some("p/x.inc")),
    ] {
        let refused = refused.map(|path| Unsafe::Writable(at(path).canonicalize().unwrap()));
        assert_eq!(check(Origin::File(at(origin))), refused.map_or(Ok(()), Err));
    }
    let missing = Origin::File(at("home/missing"));
    assert_eq!(check(missing), Err(Unsafe::Unresolved(at("home/missing"))));
    let blob = Origin::Other("blob:x".into());
    assert_eq!(check(blob), Err(Unsafe::Origin("blob:x".into())));
    assert_eq!(check(Origin::CommandLine), Ok(()));
    // `core.hooksPath` may point into the work tree: it is always overridden.
    let hooks = entry(
        Origin::File(at("home/.gitconfig")),
        "core.hookspath",
        Some("p/hooks"),
    );
    assert_eq!(check_sources(&[hooks], &bounds), Ok(()));
}

#[test]
fn unit_git_repo_rules_overrides_empty_every_program() {
    let programs = "filter.x.clean filter.x.smudge filter.x.process diff.x.command \
        diff.x.textconv diff.external merge.x.driver core.sshcommand core.pager core.editor \
        core.askpass core.gitproxy core.alternaterefscommand sequence.editor gpg.program \
        gpg.ssh.program gpg.ssh.defaultkeycommand credential.helper \
        credential.https://host.example.helper uploadpack.packobjectshook \
        remote.origin.receivepack remote.origin.uploadpack submodule.lib.update pager.log";
    let harmless = "core.autocrlf alias.st filter.x.required branch.a=b.remote user.name";
    let keys = programs
        .split_whitespace()
        .chain(harmless.split_whitespace());
    let entries: Vec<Entry> = keys
        .map(|key| entry(file("/u"), key, Some("!run")))
        .collect();
    let mut expected: Vec<String> = programs
        .split_whitespace()
        .map(|k| format!("{k}="))
        .collect();
    expected.sort();
    assert_eq!(emptied(&entries, None).unwrap(), expected);

    let all = overrides(&[], &setup(None, true)).unwrap();
    let global = ["--no-pager", "-c", "core.fsmonitor=false", "-c"].map(OsString::from);
    assert_eq!(all.global[..4], global);
    assert_eq!(all.global[4], "core.hooksPath=/data/git/empty-hooks");
    assert_eq!(
        all.command,
        ["--no-ext-diff", "--no-textconv"].map(OsString::from)
    );
}

#[test]
fn unit_git_repo_rules_overrides_keep_only_standard_lfs_and_refuse_equals() {
    let lfs = |key: &str, value: &str| entry(file("/home/.gitconfig"), key, Some(value));
    let standard = [
        lfs("filter.lfs.clean", "git-lfs clean -- %f"),
        lfs("filter.lfs.smudge", "git-lfs smudge -- %f"),
        lfs("filter.lfs.process", "git-lfs filter-process"),
        lfs("filter.lfs.required", "true"),
    ];
    let program = Path::new("/opt/it's/git-lfs");
    let rewritten = [
        "filter.lfs.clean='/opt/it'\\''s/git-lfs' clean -- %f",
        "filter.lfs.process='/opt/it'\\''s/git-lfs' filter-process",
        "filter.lfs.smudge='/opt/it'\\''s/git-lfs' smudge -- %f",
    ];
    assert_eq!(emptied(&standard, Some(program)).unwrap(), rewritten);
    // Without git-lfs, or with a value of the repository's own, they go.
    let gone = [
        "filter.lfs.clean=",
        "filter.lfs.process=",
        "filter.lfs.smudge=",
    ];
    assert_eq!(emptied(&standard, None).unwrap(), gone);
    let mut changed = standard.to_vec();
    let own = Some("git-lfs clean -- %f; x");
    changed.push(entry(file("/repo/.git/config"), "filter.lfs.clean", own));
    assert_eq!(
        emptied(&changed, Some(program)).unwrap()[0],
        "filter.lfs.clean="
    );

    // Filters that lost a program and require one; git-lfs kept its own.
    let mut entries = standard.to_vec();
    for (key, value) in [
        ("filter.crypt.smudge", Some("git-crypt smudge")),
        ("filter.crypt.required", None),
        ("filter.soft.clean", Some("soft")),
        ("filter.soft.required", Some("false")),
        ("filter.bare.required", Some("1")),
    ] {
        entries.push(entry(file("/repo/.git/config"), key, value));
    }
    let required = overrides(&entries, &setup(Some(program), false))
        .unwrap()
        .required;
    assert_eq!(required, ["crypt"]);

    // `-c 'filter.a=b.clean='` would set `filter.a`: refused.
    let equals = [entry(
        file("/repo/.git/config"),
        "filter.a=b.clean",
        Some("x"),
    )];
    let refused = Unsafe::Unnullable("filter.a=b.clean".into());
    assert_eq!(overrides(&equals, &setup(None, false)), Err(refused));
}
