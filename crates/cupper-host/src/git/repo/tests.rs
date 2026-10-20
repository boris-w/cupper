//! IT-git-repo-tier-malicious: real git on repositories made in a temp dir;
//! the machine's global and system configuration stay out of reach (`HOME`
//! is the temp dir's, `GIT_CONFIG_NOSYSTEM`). Every program the
//! configuration names is `mark <name>`, which leaves a marker file.

use std::fs;
use std::time::Duration;

use cupper_sys::Command;

use super::*;
use crate::clock::ManualClock;
use crate::driver::probe::testkit::script;
use crate::git::GitTool;
use crate::git::run::run;
use crate::store::testutil::TempDir;

const SETUP: &str = r#"
set -e
mkdir -p home markers lfs-ran
git config --global user.name test && git config --global user.email test@example.invalid
repo() { git init -q -b main "$1" && cd "$1" && printf "$2" > .gitattributes && printf 'a\n' > a.txt &&
  printf 'b\n' > a.bin && git add -A && git commit -q -m init && touch -t 202001010000 a.txt a.bin && cd ..; }
cfg() { git config -f "$1/.git/config" "$2" "$3"; }
repo evil '* filter=evil diff=evil merge=evil\n'; repo include ''; repo eq ''
repo required '* filter=req\n'; repo changing ''; repo lfs '*.bin filter=lfs\n'
mkdir -p evil/hooks
for hook in post-checkout post-index-change reference-transaction pre-auto-gc fsmonitor-watchman; do
  printf '#!/bin/sh\nMARK hook-%s\n' $hook | tee evil/hooks/$hook > evil/.git/hooks/$hook
  chmod +x evil/hooks/$hook evil/.git/hooks/$hook
done
while read key name; do cfg evil "$key" "MARK $name"; done <<EOF
core.fsmonitor fsmonitor
core.sshCommand ssh
core.pager pager
core.editor editor
core.askPass askpass
core.gitProxy proxy
core.alternateRefsCommand alternate-refs
diff.external diff-external
diff.evil.command diff-command
diff.evil.textconv textconv
filter.evil.clean clean
filter.evil.smudge smudge
filter.evil.process process
merge.evil.driver merge
sequence.editor sequence
gpg.program gpg
gpg.ssh.program gpg-ssh
gpg.ssh.defaultKeyCommand gpg-key
credential.helper credential
credential.https://example.invalid.helper credential-url
uploadpack.packObjectsHook pack-objects
remote.origin.receivepack receive-pack
remote.origin.uploadpack upload-pack
pager.log pager-log
EOF
cfg evil core.hooksPath hooks; cfg evil remote.origin.url /nonexistent
cfg evil submodule.s.update '!MARK submodule'
printf '[filter "evil"]\n  clean = MARK included\n' > include/inc.cfg; cfg include include.path ../inc.cfg
printf '[filter "a=b"]\n  clean = MARK eq\n' >> eq/.git/config
cfg required filter.req.clean 'MARK required'; cfg required filter.req.required true
cfg lfs filter.lfs.clean 'git-lfs clean -- %f'; cfg lfs filter.lfs.smudge 'git-lfs smudge -- %f'
"#;

struct Fixture {
    dir: TempDir,
    git: PathBuf,
    clock: ManualClock,
    tool: GitTool,
}

impl Fixture {
    /// The repositories of [`SETUP`], made with the git on the test's PATH.
    async fn new() -> Self {
        let path = std::env::var_os("PATH").expect("PATH");
        let mut found = std::env::split_paths(&path).map(|dir| dir.join("git"));
        let git = found.find(|git| git.is_file()).expect("git on PATH");
        let dir = TempDir::new("git-repo-tier");
        let tool = tool(dir.path(), &git, None);
        let fixture = Self {
            dir,
            git,
            clock: ManualClock::new(),
            tool,
        };
        let root = fixture.dir.path();
        let touch = format!("/usr/bin/touch '{}/markers/'\"$1\"", root.display());
        let setup = SETUP.replace("MARK", &script(root, "mark", &touch).display().to_string());
        let mut sh = Command::new("/bin/sh");
        sh.args(["-c", &setup])
            .current_dir(root)
            .env(fixture.tool.env(true));
        let out = run(&fixture.clock, sh, None, Duration::from_secs(60))
            .await
            .expect("setup");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        fixture
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.dir.path().join(relative)
    }

    /// `op` with `args` in `repo`, guarded; the repository is a project.
    async fn call(&self, repo: &str, op: RepoOp, args: &[&str]) -> Result<Done, GitError> {
        let (repo, durations) = (self.path(repo), Durations::default());
        let tier = self.tool.repo_tier(&repo);
        let cmd = GitCmd::repo(op).args(args);
        tier.run(&self.clock, &durations, std::slice::from_ref(&repo), cmd)
            .await
    }

    fn markers(&self, dir: &str) -> Vec<String> {
        let entries = fs::read_dir(self.path(dir)).expect("markers");
        let names = entries.map(|e| e.expect("entry").file_name().to_string_lossy().into());
        let mut names: Vec<String> = names.collect();
        names.sort();
        names
    }
}

/// The host's git setup, kept off the machine's system configuration.
fn tool(root: &Path, git: &Path, lfs: Option<PathBuf>) -> GitTool {
    let trusted: [PathBuf; 3] = [
        git.parent().expect("a dir").into(),
        "/usr/bin".into(),
        "/bin".into(),
    ];
    let home = Some(root.join("home"));
    let mut tool =
        GitTool::new(git.into(), lfs, &trusted, home, None, &root.join("data")).expect("tool");
    tool.test_env.push(("GIT_CONFIG_NOSYSTEM", "1".into()));
    tool
}

/// `git.unsafe_repo_config`: the index's way into the repo tier records the
/// refusal, with the file in the work tree it came from.
#[tokio::test]
async fn unit_audit_producers_git_of_records_a_refused_repo_config() {
    let fx = Fixture::new().await;
    let (audit, mut records) = tokio::sync::mpsc::unbounded_channel();
    let repo = fs::canonicalize(fx.path("include")).expect("include");
    let cx = crate::index::ScanContext {
        git: Some(fx.tool.clone()),
        writable: vec![repo.clone()],
        clock: std::sync::Arc::new(fx.clock.clone()),
        durations: Durations::default(),
        audit,
    };
    assert!(crate::index::git_of(&repo, &cx).await.is_err());
    let inc = repo.join("inc.cfg").display().to_string();
    let formal = records.try_recv().expect("a record").event.formal();
    let detail = serde_json::json!({"command": "rev_parse", "reason": "writable", "origin": inc});
    assert_eq!(
        (formal.event.as_str(), formal.detail),
        ("git.unsafe_repo_config", detail)
    );
}

#[tokio::test]
async fn it_git_repo_tier_malicious_no_program_of_the_repository_runs() {
    let fx = Fixture::new().await;
    let worktree = fx.path("worktree").display().to_string();
    let calls: [(RepoOp, &[&str]); 5] = [
        (RepoOp::RevParse, &["HEAD"]),
        (RepoOp::ForEachRef, &[]),
        (RepoOp::BranchList, &[]),
        (RepoOp::RefreshIndex, &[]),
        (RepoOp::WorktreeAdd, &[&worktree, "-b", "side"]),
    ];
    for (op, args) in calls {
        let done = fx
            .call("evil", op, args)
            .await
            .expect("runs with the overrides");
        assert_eq!(done.notice, None);
    }
    assert!(
        fx.path("worktree/a.txt").is_file(),
        "the worktree was checked out"
    );

    // Refused before git runs: an include from the work tree, a key `-c`
    // cannot empty.
    let include = fx
        .call("include", RepoOp::RevParse, &[])
        .await
        .expect_err("refused");
    let inc = fs::canonicalize(fx.path("include/inc.cfg")).expect("inc.cfg");
    assert!(matches!(&include, GitError::Unsafe(Unsafe::Writable(path)) if *path == inc));
    let notice = include.notice().expect("a notice");
    assert_eq!(
        (notice.code, notice.paths),
        (NoticeCode::UnsafeRepoConfig, vec![inc])
    );
    let eq = fx.call("eq", RepoOp::RevParse, &[]).await;
    assert!(
        matches!(eq, Err(GitError::Unsafe(Unsafe::Unnullable(_)))),
        "{eq:?}"
    );

    // A required filter without its program fails the call, which is not
    // retried; an ordinary failure stays one.
    let required = fx
        .call("required", RepoOp::RefreshIndex, &[])
        .await
        .expect_err("required");
    assert!(
        matches!(&required, GitError::RequiredFilter(f) if *f == ["req"]),
        "{required:?}"
    );
    assert_eq!(required.reason(), Some(ErrorReason::UnsafeRepoConfig));
    let missing = fx
        .call("evil", RepoOp::RevParse, &["--verify", "no-such-ref"])
        .await;
    let missing = missing.expect_err("no such ref");
    assert!(matches!(missing, GitError::Failed { ref changed, .. } if changed.is_empty()));
    assert_eq!(
        (missing.reason(), missing.code()),
        (None, ErrorCode::GitError)
    );
    assert_eq!(fx.markers("markers"), Vec::<String>::new());
}

#[tokio::test]
async fn it_git_repo_tier_malicious_config_changed_during_the_call() {
    let mut fx = Fixture::new().await;
    // git, then a line more in the configuration, except when reading it.
    let config = fx.path("changing/.git/config");
    let (git, file) = (fx.git.display(), config.display());
    let body = format!(
        "'{git}' \"$@\"; s=$?\ncase \"$*\" in *' config --list '*) ;; *) echo '[x]' >> '{file}';; esac\nexit $s"
    );
    let wrapper = script(fx.dir.path(), "git", &body);
    fx.tool = tool(fx.dir.path(), &wrapper, None);
    let config = vec![fs::canonicalize(&config).expect("config")];
    let read = fx
        .call("changing", RepoOp::RevParse, &["HEAD"])
        .await
        .expect_err("dropped");
    assert!(
        matches!(&read, GitError::ConfigChanged(paths) if *paths == config),
        "{read:?}"
    );
    assert_eq!(
        read.notice().map(|n| n.code),
        Some(NoticeCode::RepoConfigChanged)
    );
    let write = fx
        .call("changing", RepoOp::RefreshIndex, &[])
        .await
        .expect("the write stands");
    let notice = write.notice.expect("a notice");
    assert_eq!(
        (notice.code, notice.paths),
        (NoticeCode::RepoConfigChanged, config)
    );
}

#[tokio::test]
async fn it_git_repo_tier_malicious_lfs_filters_run_the_hosts_git_lfs() {
    let mut fx = Fixture::new().await;
    let body = format!(
        "/usr/bin/touch '{}/lfs-ran/'\"$1\"; /bin/cat",
        fx.dir.path().display()
    );
    let lfs = script(fx.dir.path(), "git-lfs", &body);
    fx.tool = tool(fx.dir.path(), &fx.git, Some(lfs));
    fx.call("lfs", RepoOp::RefreshIndex, &[])
        .await
        .expect("clean");
    let worktree = fx.path("lfs-worktree").display().to_string();
    fx.call("lfs", RepoOp::WorktreeAdd, &[&worktree, "-b", "side"])
        .await
        .expect("smudge");
    assert_eq!(
        fs::read(fx.path("lfs-worktree/a.bin")).expect("checked out"),
        b"b\n"
    );
    assert_eq!(fx.markers("lfs-ran"), ["clean", "smudge"]);
}
