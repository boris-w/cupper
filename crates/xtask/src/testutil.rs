//! Helpers shared by the unit tests.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A fresh directory under the system temp dir, removed on drop.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("xtask-test-{}-{unique}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Runs git in `dir` without the user's global or system configuration.
pub fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=xtask",
            "-c",
            "user.email=xtask@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A temporary repository with one commit and a clean work tree.
pub fn clean_repo(name: &str) -> TempDir {
    let dir = TempDir::new(name);
    git(dir.path(), &["init", "--quiet"]);
    std::fs::write(dir.path().join("README"), "fixture\n").expect("write file");
    git(dir.path(), &["add", "README"]);
    git(dir.path(), &["commit", "--quiet", "-m", "fixture"]);
    dir
}

const ALONE_ENV: &str = "XTASK_TEST_ALONE";

/// Runs the test `path` (`module::tests::name`) in a test process of its own,
/// where no other test starts a process meanwhile: until it execs, a process
/// started on another thread holds a copy of every fd of this one, a `flock`
/// included (as `cupper-sys`'s `alone`). Returns `true` in that process,
/// where the caller goes on with the test body; in the calling process it
/// runs that process, checks that exactly this test ran and passed, and
/// returns `false`.
pub fn alone(path: &str) -> bool {
    if std::env::var_os(ALONE_ENV).is_some_and(|name| name == path) {
        return true;
    }
    let out = Command::new(std::env::current_exe().expect("test binary"))
        .args([path, "--exact", "--test-threads=1"])
        .env(ALONE_ENV, path)
        .stdin(Stdio::null())
        .output()
        .expect("run the test alone");
    let text = String::from_utf8_lossy(&out.stdout) + String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{path} failed on its own:\n{text}");
    assert!(
        text.contains("test result: ok. 1 passed"),
        "{path} did not run on its own:\n{text}"
    );
    false
}
