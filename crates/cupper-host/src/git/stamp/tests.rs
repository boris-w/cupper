//! UNIT-git-fingerprint: each kind of content the fingerprint covers, in a
//! repository and a linked worktree laid out by hand in a temporary
//! directory.

use super::*;
use crate::store::testutil::TempDir;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

fn entry(origin: PathBuf, key: &str, value: &str) -> Entry {
    let (key, value) = (key.as_bytes().to_vec(), Some(value.as_bytes().to_vec()));
    let origin = Origin::File(origin);
    Entry { origin, key, value }
}

#[test]
fn unit_git_fingerprint_sees_each_kind_of_content_and_nothing_else() {
    let dir = TempDir::new("git-stamp");
    let at = |path: &str| dir.path().canonicalize().unwrap().join(path);
    let files = "w/.git/config w/.git/HEAD w/.git/hooks/pre-commit w/.git/info/attributes \
        w/.gitattributes w/src/.gitattributes w/src/main.rs w/my-hooks/post-checkout \
        home/.gitconfig home/inc";
    for path in files.split_whitespace() {
        write(&at(path), "1");
    }
    let layout = Layout::read(&at("w")).unwrap();
    assert_eq!(
        (&layout.git_dir, &layout.common_dir),
        (&at("w/.git"), &at("w/.git"))
    );
    let config = [
        entry(at("home/.gitconfig"), "include.path", "inc"),
        entry(at("home/inc"), "core.hookspath", "my-hooks"),
        entry(at("w/.git/config"), "core.bare", "false"),
    ];
    let take = || Fingerprint::take(&layout, &config).unwrap();
    let mut before = take();
    // Changing each kind shows its path, and only it.
    let changes = "w/.git/config home/.gitconfig home/inc w/.git/config.worktree \
        w/.git/hooks/pre-commit w/.git/hooks/post-merge w/my-hooks/post-checkout \
        w/.git/info/attributes w/.gitattributes w/src/deep/.gitattributes";
    for path in changes.split_whitespace() {
        write(&at(path), "2");
        let after = take();
        assert_eq!(before.changed(&after), [at(path)], "{path}");
        before = after;
    }
    fs::remove_file(at("w/.git/hooks/pre-commit")).unwrap();
    assert_eq!(before.changed(&take()), [at("w/.git/hooks/pre-commit")]);
    before = take();
    // Unrelated files, git's own state, and attributes inside `.git`.
    let unrelated = "w/src/main.rs w/notes.txt w/.git/HEAD w/.git/sub/.gitattributes";
    for path in unrelated.split_whitespace() {
        write(&at(path), "3");
    }
    assert_eq!(before.changed(&take()), Vec::<PathBuf>::new());
}

#[test]
fn unit_git_fingerprint_reads_a_linked_worktree() {
    let dir = TempDir::new("git-stamp-worktree");
    let at = |path: &str| dir.path().canonicalize().unwrap().join(path);
    for path in ["main/.git/config", "main/.git/hooks/h", "wt/src/a.rs"] {
        write(&at(path), "1");
    }
    write(&at("main/.git/worktrees/w/commondir"), "../..\n");
    write(&at("wt/.git"), "gitdir: ../main/.git/worktrees/w\n");
    let layout = Layout::read(&at("wt")).unwrap();
    let dirs = (&layout.git_dir, &layout.common_dir);
    assert_eq!(dirs, (&at("main/.git/worktrees/w"), &at("main/.git")));
    let config = [entry(at("main/.git/config"), "core.bare", "false")];
    let mut before = Fingerprint::take(&layout, &config).unwrap();
    let changes = "main/.git/worktrees/w/config.worktree main/.git/config main/.git/hooks/h \
        wt/.gitattributes";
    for path in changes.split_whitespace() {
        write(&at(path), "2");
        let after = Fingerprint::take(&layout, &config).unwrap();
        assert_eq!(before.changed(&after), [at(path)], "{path}");
        before = after;
    }
}
