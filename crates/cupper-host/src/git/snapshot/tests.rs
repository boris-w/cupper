//! The snapshot tier on real git, on repositories made in a temp dir; the
//! machine's global and system configuration stay out of reach (`HOME` is
//! the temp dir's, `GIT_CONFIG_NOSYSTEM`). The reference is SP-5's way: the
//! user's own git on a copy of the index, writing to an object store of its
//! own.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use cupper_sys::Command;
use sha2::{Digest, Sha256};

use super::*;
use crate::clock::{ManualClock, SystemClock};
use crate::driver::probe::testkit::script;
use crate::git::repo::tests::{Fixture, tool};
use crate::git::run::run;
use crate::store::testutil::TempDir;

/// Before each setup: the user's identity, `mk` for a repository, `MARK
/// <name>` for a program that leaves `markers/<name>`, and SP-5's snapshot
/// as `reference <repo>`.
const PRELUDE: &str = r#"
set -e
T=$PWD
mkdir -p home markers
git config --global user.name test && git config --global user.email test@example.invalid
mk() { git init -q -b main "$@"; }
reference() { (cd "$1" && i=$(git rev-parse --git-path index) &&
  o=$(cd "$(git rev-parse --git-path objects)" && pwd) && rm -rf "$T/ref" && mkdir -p "$T/ref/objects" &&
  { [ ! -f "$i" ] || cp -p "$i" "$T/ref/index"; } &&
  export GIT_INDEX_FILE="$T/ref/index" GIT_OBJECT_DIRECTORY="$T/ref/objects" GIT_ALTERNATE_OBJECT_DIRECTORIES="$o" &&
  git -c core.safecrlf=false -c advice.addEmbeddedRepo=false add -A --ignore-errors && git write-tree); }
"#;

struct Repos {
    dir: TempDir,
    clock: ManualClock,
    tool: GitTool,
}

impl Repos {
    /// The repositories `setup` makes, after [`PRELUDE`]; the host's git-lfs
    /// is `lfs`.
    async fn new(name: &str, setup: &str, lfs: Option<PathBuf>) -> Self {
        let path = std::env::var_os("PATH").expect("PATH");
        let mut found = std::env::split_paths(&path).map(|dir| dir.join("git"));
        let git = found.find(|git| git.is_file()).expect("git on PATH");
        let dir = TempDir::new(name);
        let tool = tool(dir.path(), &git, lfs);
        let repos = Self {
            dir,
            clock: ManualClock::new(),
            tool,
        };
        let touch = format!(
            "/usr/bin/touch '{}/markers/'\"$1\"",
            repos.dir.path().display()
        );
        let mark = script(repos.dir.path(), "mark", &touch)
            .display()
            .to_string();
        repos.sh(&setup.replace("MARK", &mark)).await;
        repos
    }

    /// What `script` prints, after [`PRELUDE`].
    async fn sh(&self, script: &str) -> String {
        let mut sh = Command::new("/bin/sh");
        sh.args(["-c", &format!("{PRELUDE}\n{script}")])
            .current_dir(self.dir.path())
            .env(self.tool.env(true));
        let out = run(&self.clock, sh, None, Duration::from_secs(60)).await;
        let out = out.expect("runs");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "{script}: {err}");
        // Trees are binary.
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    async fn take(&self, repo: &str, start: Option<&str>) -> Taken {
        let work_tree = self.dir.path().join(repo);
        let durations = Durations::default();
        let taken = take(&self.tool, &work_tree, start, &self.clock, &durations).await;
        taken.expect("a snapshot")
    }

    /// `git <args>` in `repo`, the snapshots' objects in reach.
    async fn git(&self, repo: &str, args: &str) -> String {
        let objects = "$(printf '%s:' \"$T\"/data/git/*/objects)";
        let script = format!("GIT_ALTERNATE_OBJECT_DIRECTORIES={objects} git -C {repo} {args}");
        self.sh(&script).await
    }
}

/// The paths a snapshot left out, with why.
fn excluded(taken: &Taken) -> Vec<(String, String)> {
    let shown = |e: &ExcludedPath| (e.path.0.clone(), e.reason.as_str().to_owned());
    taken.excluded.iter().map(shown).collect()
}

const SHAPES: &str = r#"
mk basic && cd basic && printf '*.log\n' > .gitignore && printf 'a\n' > a && printf 'b\n' > b &&
  printf 'k\n' > kept.log && git add -A && git add -f kept.log && git commit -qm init &&
  printf 'a2\n' > a && rm b && printf 'c\n' > c && printf 'x\n' > x.log && printf 'k2\n' > kept.log &&
  printf 'excl.txt\n' >> .git/info/exclude && printf 'e\n' > excl.txt && cd ..
mk crlf && cd crlf && git config core.autocrlf true && printf 'a\r\nb\r\n' > w.txt && git add -A &&
  git commit -qm init && printf 'c\r\n' >> w.txt && cd ..
mk split && cd split && git config core.splitIndex true && printf 'a\n' > a && git add a &&
  git commit -qm init && git update-index --split-index && printf 'b\n' > b && git add b &&
  printf 'a2\n' > a && cd ..
mk sparse && cd sparse && mkdir in out && printf 'i\n' > in/f && printf 'o\n' > out/f && git add -A &&
  git commit -qm init && git sparse-checkout set --cone --sparse-index in && printf 'i2\n' > in/f &&
  printf 'n\n' > in/new && cd ..
mk untr && cd untr && git config core.untrackedCache true && printf 'a\n' > a && git add a &&
  git commit -qm init && git update-index --untracked-cache && printf 'u\n' > u &&
  git status -unormal > /dev/null && cd ..
git -C basic worktree add -q ../linked -b side && printf 'l\n' > linked/a && printf 'n\n' > linked/new
git init -q -b main --object-format=sha256 sha && cd sha && printf 'a\n' > a && git add a &&
  git commit -qm init && printf 'a2\n' > a && cd ..
mk excl && cd excl && printf 'a\n' > a && git add a && git commit -qm init &&
  dd if=/dev/zero of=big bs=1 count=0 seek=52428801 2> /dev/null && git init -q vendor/lib &&
  printf 'v\n' > vendor/lib/v && mkdir gen && i=0 && while [ $i -le 10000 ]; do : > gen/f$i; i=$((i+1)); done
"#;

#[tokio::test]
async fn it_git_snapshot_parity_trees_match_the_users_git_in_every_index_shape() {
    let fx = Repos::new("snapshot-parity", SHAPES, None).await;
    for repo in ["basic", "crlf", "split", "sparse", "untr", "linked", "sha"] {
        let taken = fx.take(repo, None).await;
        let reference = fx.sh(&format!("reference {repo}")).await;
        assert_eq!(taken.tree, reference, "{repo}");
        assert!(taken.excluded.is_empty() && !taken.partial, "{repo}");
    }
    // A large untracked file, a nested repository and a large untracked
    // directory stay out; the rest is in.
    let taken = fx.take("excl", None).await;
    let want = [
        ("gen", "large_untracked_dir"),
        ("big", "too_large"),
        ("vendor/lib", "nested_repo"),
    ];
    let want: Vec<(String, String)> = want.map(|(p, r)| (p.into(), r.into())).into();
    assert_eq!(excluded(&taken), want);
    let names = format!("ls-tree --name-only {}", taken.tree);
    assert_eq!(fx.git("excl", &names).await, "a");
    // The end's change against the start.
    fx.sh("printf 'a\\nb\\n' > excl/a").await;
    let end = fx.take("excl", Some(&taken.tree)).await;
    let stat = DiffStat {
        files: 1,
        added: 1,
        removed: 0,
    };
    assert_eq!(end.stat, Some(stat));
}

#[tokio::test]
async fn it_git_racy_a_file_changed_within_the_index_second_is_snapshotted() {
    // The file and the index share one mtime, the file's size stays: only
    // the index's own mtime tells git to look again.
    let setup =
        "mk racy && cd racy && printf 'aaaa\\n' > a && touch -t 202001010000 a && git add a &&
        git commit -qm init && printf 'bbbb\\n' > a && touch -t 202001010000 a .git/index";
    let fx = Repos::new("snapshot-racy", setup, None).await;
    let taken = fx.take("racy", None).await;
    let blob = fx.git("racy", &format!("rev-parse {}:a", taken.tree)).await;
    assert_eq!(
        blob,
        fx.sh("printf 'bbbb\\n' | git hash-object --stdin").await
    );

    // The copy keeps the mtime.
    let (from, to) = (fx.dir.path().join("racy/a"), fx.dir.path().join("copied"));
    copy(&from, &to).expect("copied");
    let mtime = |path: &Path| {
        fs::metadata(path)
            .and_then(|m| m.modified())
            .expect("mtime")
    };
    assert_eq!(mtime(&to), mtime(&from));
    // 2020, as the setup made it, not now.
    assert!(mtime(&to) < SystemTime::UNIX_EPOCH + Duration::from_secs(1_609_459_200));
}

#[tokio::test]
async fn it_git_filter_placeholder_a_filter_declared_only_in_info_attributes() {
    let setup = "mk crypt && cd crypt && printf 'a\\n' > a && git add a && git commit -qm init &&
        git config filter.crypt.clean 'MARK clean' && git config filter.crypt.smudge 'MARK smudge' &&
        printf 'secret.txt filter=crypt\\n' > .git/info/attributes && printf 'plaintext-secret\\n' > secret.txt";
    let fx = Repos::new("snapshot-filter", setup, None).await;
    let taken = fx.take("crypt", None).await;
    let want = vec![("secret.txt".to_owned(), "filter_unsupported".to_owned())];
    assert_eq!(excluded(&taken), want);
    let show = format!("cat-file -p {}:secret.txt", taken.tree);
    let placeholder = fx.git("crypt", &show);
    let hash: String = Sha256::digest(b"plaintext-secret\n")
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        placeholder.await,
        format!("cupper: not stored, filter crypt not run; sha-256 {hash}")
    );
    // Not one object of the snapshots holds the plaintext; no filter ran.
    let all = "for o in data/git/*/objects; do GIT_DIR=crypt/.git GIT_OBJECT_DIRECTORY=$o \
        git cat-file --batch-all-objects --batch; done";
    assert!(!fx.sh(all).await.contains("plaintext-secret"));
    assert!(
        fs::read_dir(fx.dir.path().join("markers"))
            .expect("markers")
            .next()
            .is_none()
    );
}

/// Q32: on the repo tier's malicious repositories, where every key that
/// names a program, the hooks, an include from the work tree and
/// `info/attributes` leave a marker, snapshots and their change run none.
#[tokio::test]
async fn it_git_snapshot_malicious_no_program_of_the_repository_runs() {
    let fx = Fixture::new().await;
    let durations = Durations::default();
    for repo in ["evil", "include", "required", "lfs"] {
        let work_tree = fx.path(repo);
        let start = take(&fx.tool, &work_tree, None, &fx.clock, &durations).await;
        let start = start.expect("a snapshot");
        fs::write(work_tree.join("a.txt"), "changed\n").expect("written");
        let end = take(
            &fx.tool,
            &work_tree,
            Some(&start.tree),
            &fx.clock,
            &durations,
        )
        .await;
        assert!(end.expect("a snapshot").stat.is_some(), "{repo}");
    }
    assert_eq!(fx.markers("markers"), Vec::<String>::new());
}

/// IT-git-lfs-snapshot (Q32): with the user's standard git-lfs filters, the
/// snapshot tier runs the host's git-lfs by its absolute path, which its
/// PATH lacks, and stores the pointer the user's git does.
#[tokio::test]
async fn it_git_lfs_snapshot_runs_the_hosts_git_lfs() {
    let lfs = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/git-lfs/git-lfs");
    let Ok(lfs) = lfs.canonicalize() else {
        eprintln!("skipped: no git-lfs; `cargo xtask git-lfs` downloads the pinned one");
        return;
    };
    let path = format!(
        "export PATH='{}':$PATH",
        lfs.parent().expect("a dir").display()
    );
    let setup = format!(
        "{path}; mk lfs && cd lfs && git lfs install --local > /dev/null &&
        printf '*.bin filter=lfs diff=lfs merge=lfs -text\\n' > .gitattributes &&
        printf 'one\\n' > a.bin && git add -A && git commit -qm init && printf 'two\\n' > a.bin"
    );
    let fx = Repos::new("snapshot-lfs", &setup, Some(lfs)).await;
    let taken = fx.take("lfs", None).await;
    assert!(taken.excluded.is_empty() && !taken.partial);
    assert_eq!(taken.tree, fx.sh(&format!("{path}; reference lfs")).await);
    let pointer = format!("cat-file -p {}:a.bin", taken.tree);
    let pointer = fx.git("lfs", &pointer).await;
    assert!(
        pointer.starts_with("version https://git-lfs.github.com/spec/v1"),
        "{pointer}"
    );
}

/// PERF-snapshot: a synthetic repository of 10,000 files, snapshotted with
/// no change and with 10 files changed; the median of five of each.
#[tokio::test]
#[ignore = "PERF-snapshot runs nightly"]
async fn perf_snapshot_ten_thousand_files() {
    let setup =
        "mk big && cd big && i=0 && while [ $i -lt 10000 ]; do echo $i > f$i; i=$((i+1)); done &&
        git add -A && git commit -qm init";
    let fx = Repos::new("snapshot-perf", setup, None).await;
    let clock = SystemClock::new();
    for (case, change) in [("no change", false), ("10 files changed", true)] {
        let mut runs = Vec::new();
        for n in 0..5 {
            if change {
                let edit =
                    format!("i=0; while [ $i -lt 10 ]; do echo {n} >> big/f$i; i=$((i+1)); done");
                fx.sh(&edit).await;
            }
            let start = clock.now_mono();
            fx.take("big", None).await;
            runs.push(clock.now_mono().saturating_duration_since(start));
        }
        runs.sort();
        let metric = format!("snapshot_10k_{}", case.replace(' ', "_").replace(',', ""));
        let ms: Vec<f64> = runs.iter().map(|run| run.as_secs_f64() * 1000.0).collect();
        crate::perf::record(&metric, "ms", &ms);
        assert!(runs[2] < Duration::from_secs(2), "{case}: {runs:?}");
    }
}
