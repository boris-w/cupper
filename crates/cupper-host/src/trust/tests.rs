//! UNIT-trust-scan: the range, links, limits, digest, states and diff of the
//! trust scan, on trees this test builds in fresh temporary directories.

use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use cupper_protocol::ids::{Digest, PrincipalId};
use cupper_protocol::index::TrustState;
use cupper_protocol::item::DiffChange;
use cupper_protocol::methods::project::TrustedFileChange;
use sha2::{Digest as _, Sha256};

use super::*;

/// A fresh directory, removed on drop.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "cupper-trust-{}-{unique}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("ws")).unwrap();
        Self(dir)
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.0.join(rel)
    }

    fn real(&self, rel: &str) -> PathBuf {
        fs::canonicalize(self.path(rel)).unwrap()
    }

    fn file(&self, rel: &str, content: &str) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn link(&self, rel: &str, target: &str) {
        let path = self.path(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        symlink(self.path(target), path).unwrap();
    }

    fn scan(&self) -> Result<Scan, ScanError> {
        scan(&self.path("ws"), None)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn paths(scan: &Scan) -> Vec<&str> {
    scan.files.iter().map(|file| file.path.as_str()).collect()
}

fn change(path: &str, change: DiffChange) -> TrustedFileChange {
    let path = cupper_protocol::annotations::WirePath(path.into());
    TrustedFileChange { path, change }
}

#[test]
fn unit_trust_scan_range_covers_each_level_up_to_the_git_root() {
    let t = Tree::new("range");
    t.file("repo/.codex/config.toml", "model = 'o3'");
    t.file("repo/app/.mcp.json", "{}");
    t.file("repo/app/web/.claude/settings.json", "{}");
    t.file("repo/app/web/.claude/agents/review.md", "review");
    // Below the workspace and above the git root: out of range.
    t.file("repo/app/web/src/.claude/settings.json", "{}");
    t.file(".claude/settings.json", "{}");
    let workspace = t.path("repo/app/web");

    let scan = scan(&workspace, Some(&t.path("repo"))).unwrap();
    let expected = [
        "../../.codex/config.toml",
        "../.mcp.json",
        ".claude/agents/review.md",
        ".claude/settings.json",
    ];
    assert_eq!(paths(&scan), expected);
    let codex = &scan.files[0];
    assert_eq!(codex.target, t.real("repo/.codex/config.toml"));
    assert_eq!(codex.content, b"model = 'o3'");
    assert!(scan.files.iter().all(|f| !f.linked && !f.outside_workspace));

    // Outside git, the workspace alone.
    let alone = super::scan(&workspace, None).unwrap();
    assert_eq!(paths(&alone), &expected[2..]);

    let err = super::scan(&t.path("repo"), Some(&workspace)).unwrap_err();
    assert_eq!(err.code(), "root_not_ancestor", "{err}");
}

#[test]
fn unit_trust_scan_links_are_followed_to_their_targets() {
    let t = Tree::new("links");
    t.file("dotfiles/mcp.json", r#"{"mcpServers": {}}"#);
    t.file("ws/shared/skills/lint/SKILL.md", "lint");
    t.link("ws/.mcp.json", "dotfiles/mcp.json");
    t.link("ws/.claude/skills", "ws/shared/skills");

    let scan = t.scan().unwrap();
    assert_eq!(paths(&scan), [".claude/skills/lint/SKILL.md", ".mcp.json"]);
    let (skill, mcp) = (&scan.files[0], &scan.files[1]);
    assert_eq!(skill.target, t.real("ws/shared/skills/lint/SKILL.md"));
    assert!(skill.linked && !skill.outside_workspace);
    assert_eq!(mcp.target, t.real("dotfiles/mcp.json"));
    assert!(mcp.linked && mcp.outside_workspace);
    assert_eq!(mcp.content, br#"{"mcpServers": {}}"#);

    let fails = |build: &dyn Fn(&Tree)| {
        let t = Tree::new("broken");
        build(&t);
        t.scan().unwrap_err().code()
    };
    assert_eq!(
        fails(&|t| t.link("ws/.claude/gone", "nowhere")),
        "dangling_link"
    );
    assert_eq!(
        fails(&|t| t.link("ws/.claude/up", "ws/.claude")),
        "link_cycle"
    );
    let pair = |t: &Tree| {
        t.link("ws/.mcp.json", "ws/other");
        t.link("ws/other", "ws/.mcp.json");
    };
    assert_eq!(fails(&pair), "link_cycle");
    let socket = |t: &Tree| {
        fs::create_dir(t.path("ws/.codex")).unwrap();
        std::os::unix::net::UnixListener::bind(t.path("ws/.codex/sock")).unwrap();
    };
    assert_eq!(fails(&socket), "not_regular");
}

#[test]
fn unit_trust_scan_limits_count_files_and_bytes_with_link_targets() {
    let t = Tree::new("files");
    for i in 0..MAX_FILES {
        t.file(&format!("ws/.claude/f{i}"), "");
    }
    assert_eq!(t.scan().unwrap().files.len(), MAX_FILES);
    t.file("ws/.mcp.json", "{}");
    assert_eq!(t.scan().unwrap_err().code(), "too_many_files");

    let t = Tree::new("bytes");
    t.file("ws/.claude/big", "");
    let big = fs::OpenOptions::new()
        .write(true)
        .open(t.path("ws/.claude/big"))
        .unwrap();
    big.set_len(MAX_BYTES).unwrap();
    assert_eq!(t.scan().unwrap().files.len(), 1);
    // The same file again, through a link: twice the bytes.
    t.link("ws/.codex", "ws/.claude");
    assert_eq!(t.scan().unwrap_err().code(), "too_large");
}

#[test]
fn unit_trust_scan_digest_follows_content_paths_and_link_targets_only() {
    let t = Tree::new("digest");
    t.file("shared/one.json", "{}");
    t.file("shared/two.json", "{}");
    let build = |reversed: bool| {
        let _ = fs::remove_dir_all(t.path("ws"));
        let mut steps: Vec<Box<dyn Fn()>> = vec![
            Box::new(|| t.file("ws/.claude/settings.json", "a")),
            Box::new(|| t.file("ws/.claude/agents/x.md", "b")),
            Box::new(|| t.link("ws/.mcp.json", "shared/one.json")),
        ];
        if reversed {
            steps.reverse();
        }
        steps.iter().for_each(|step| step());
        t.scan().unwrap().digest
    };
    let digest = build(false);
    assert_eq!(build(true), digest, "the order of creation does not count");

    t.file("ws/.claude/settings.json", "c");
    assert_ne!(t.scan().unwrap().digest, digest, "content");
    build(false);
    fs::rename(
        t.path("ws/.claude/agents/x.md"),
        t.path("ws/.claude/agents/y.md"),
    )
    .unwrap();
    assert_ne!(t.scan().unwrap().digest, digest, "path");
    build(false);
    fs::remove_file(t.path("ws/.mcp.json")).unwrap();
    t.link("ws/.mcp.json", "shared/two.json");
    assert_ne!(t.scan().unwrap().digest, digest, "link target");

    // No file: the tag's hash, the same for every project.
    let empty = Tree::new("empty").scan().unwrap();
    let tag: [u8; 32] = Sha256::digest(b"cupper-trust-v1").into();
    assert_eq!(empty.digest, Digest::from_sha256(&tag));
    assert!(empty.files.is_empty());
}

#[test]
fn unit_trust_scan_states_and_diff_from_trusted() {
    let t = Tree::new("states");
    let by: PrincipalId = "pr_test".parse().unwrap();

    // Added with no file: trusted at once; a file appearing makes it stale.
    let empty = t.scan().unwrap();
    let auto = Trust::added(&empty, 1);
    assert_eq!(auto.state, TrustState::Trusted);
    assert_eq!(auto.trusted.as_ref().unwrap().by.as_str(), "auto_empty");
    t.file("ws/.claude/settings.json", "{}");
    let first = t.scan().unwrap();
    let appeared = auto.rescanned(&first);
    assert_eq!(appeared.state, TrustState::Stale);
    let added = change(".claude/settings.json", DiffChange::Add);
    assert_eq!(appeared.diff_from_trusted(&first), Some(vec![added]));

    // Added with a file: untrusted until confirmed with the digest seen.
    let untrusted = Trust::added(&first, 2);
    assert_eq!(untrusted.state, TrustState::Untrusted);
    assert_eq!(untrusted.rescanned(&first), untrusted);
    assert_eq!(untrusted.diff_from_trusted(&first), None);
    let mismatch = Trust::confirmed(&first, &empty.digest, by.clone(), 3);
    assert_eq!(mismatch, Err(DigestMismatch));
    let trusted = Trust::confirmed(&first, &first.digest, by, 3).unwrap();
    let record = trusted.trusted.as_ref().unwrap();
    assert_eq!((record.by.as_str(), record.at_ms), ("pr_test", 3));
    assert_eq!(record.manifest, first.manifest());
    assert_eq!(trusted.rescanned(&first), trusted);
    assert_eq!(trusted.diff_from_trusted(&first), None);

    // Changed, added, deleted.
    t.file("ws/.claude/settings.json", r#"{"env": {}}"#);
    t.file("ws/.mcp.json", "{}");
    let second = t.scan().unwrap();
    let stale = trusted.rescanned(&second);
    assert_eq!(stale.state, TrustState::Stale);
    let expected = [
        change(".claude/settings.json", DiffChange::Update),
        change(".mcp.json", DiffChange::Add),
    ];
    assert_eq!(stale.diff_from_trusted(&second).unwrap(), expected);
    fs::remove_file(t.path("ws/.claude/settings.json")).unwrap();
    let third = t.scan().unwrap();
    let deleted = change(".claude/settings.json", DiffChange::Delete);
    assert_eq!(stale.diff_from_trusted(&third).unwrap()[0], deleted);

    // Back to what was trusted: still stale until confirmed again.
    t.file("ws/.claude/settings.json", "{}");
    fs::remove_file(t.path("ws/.mcp.json")).unwrap();
    let back = t.scan().unwrap();
    assert_eq!(back.digest, first.digest);
    assert_eq!(stale.rescanned(&back).state, TrustState::Stale);
    assert_eq!(stale.diff_from_trusted(&back), Some(vec![]));

    // The manifest column: keys in order, read back as written.
    let json = second.manifest().to_json();
    assert!(json.starts_with(r#"{".claude/settings.json":""#), "{json}");
    assert!(json.find(".mcp.json") > json.find(".claude"), "{json}");
    assert_eq!(Manifest::from_json(&json).unwrap(), second.manifest());
}
