//! IT-git-hunk (GT-3): SP-5's hunk cases (`spikes/SP-5-git-snapshot/
//! edge_hunk.py`) on the product's path, `git/diff` → `git/file_diff` →
//! `git/stage`, `git/unstage` or `git/revert`, in a session's repository.
//!
//! Each scenario is a table of steps, one per SP-5 row with its label: an
//! operation on one unit of one file, and the file afterwards in the index
//! (staging) or the work tree (reverting), as the edits make it, not as git
//! does. Every step also checks that a staging left the work tree as it was
//! and a revert the index's entries. What stands in for SP-5's rows that do
//! not apply to the product is in T228's notes.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;
use std::time::Duration;

use Pick::{First, Marked, Mode};
use cupper_protocol::ids::SessionId;
use cupper_protocol::session::StopReason;
use cupper_sys::{Command, Env};
use serde_json::{Value, json};

use super::tests::{Fixture, code, done, harness};
use super::trust_tests::git_project_with;
use crate::clock::ManualClock;
use crate::driver::api::PromptTag;
use crate::git::run::run;
use crate::store::testutil::TempDir;

/// `n` lines `line NN` ending in `eol`.
fn lines(n: usize, eol: &str) -> Vec<String> {
    (0..n).map(|i| format!("line {i:02}{eol}")).collect()
}

/// A change of a list of lines: replace line `at`, insert after it, delete it.
#[derive(Debug, Clone, Copy)]
enum E {
    R(usize, &'static str),
    I(usize, &'static str),
    D(usize),
}

/// SP-5's three edits: A replaces line 2, B inserts after 15, C deletes 27.
const A: E = E::R(2, "line 02 EDIT-A\n");
const B: E = E::I(15, "EDIT-B inserted\n");
const C: E = E::D(27);

/// `base` with `edits`, bottom up so that the indexes hold; `\n` in a
/// payload becomes `eol`.
fn edit(base: &[String], edits: &[E], eol: &str) -> Vec<u8> {
    let (mut out, mut edits) = (base.to_vec(), edits.to_vec());
    let at = |e: &E| match e {
        E::R(at, _) | E::I(at, _) | E::D(at) => std::cmp::Reverse(*at),
    };
    edits.sort_by_key(at);
    for e in edits {
        match e {
            E::R(at, text) => out[at] = text.replace('\n', eol),
            E::I(at, text) => out.insert(at + 1, text.replace('\n', eol)),
            E::D(at) => drop(out.remove(at)),
        }
    }
    out.concat().into_bytes()
}

/// A file's content and mode, as the index or the work tree has it.
type Held = Option<(Vec<u8>, String)>;

fn held(bytes: impl Into<Vec<u8>>, mode: &str) -> Held {
    Some((bytes.into(), mode.into()))
}

fn text(bytes: impl Into<Vec<u8>>) -> Held {
    held(bytes, "100644")
}

/// Which unit of a file a step takes.
#[derive(Debug, Clone, Copy)]
enum Pick {
    First,
    Mode,
    /// The one with a changed line holding this.
    Marked(&'static str),
}

/// A step: SP-5's label, the operation (`stage`, `unstage`, `revert`), the
/// file, its unit, and the file afterwards in the index, or the work tree
/// for a revert.
type Step = (&'static str, &'static str, &'static str, Pick, Held);

/// The steps of case `c`, a row per line as SP-5's table has them:
/// `"<label>": <operation> <file>, <unit> => <afterwards>;`.
macro_rules! steps {
    ($c:ident; $($label:literal: $op:ident $path:expr, $pick:expr => $want:expr;)*) => {
        $c.steps(vec![$(($label, stringify!($op), $path, $pick, $want)),*]).await
    };
}

/// A repository of its own, with a session once it [`start`](Self::start)s.
pub(super) struct Case {
    pub(super) f: Fixture,
    pub(super) home: TempDir,
    git: PathBuf,
    /// The git-lfs of the user's git and the host's.
    lfs: Option<PathBuf>,
    session: Option<SessionId>,
    commands: u32,
}

impl Case {
    pub(super) async fn new(name: &str) -> Self {
        Self::with_lfs(name, None).await
    }

    /// A case whose user and host run `lfs` as git-lfs.
    pub(super) async fn with_lfs(name: &str, lfs: Option<PathBuf>) -> Self {
        let (f, home) = git_project_with(name, lfs.clone()).await;
        let path = std::env::var_os("PATH").unwrap();
        let mut found = std::env::split_paths(&path).map(|dir| dir.join("git"));
        let git = found.find(|git| git.is_file()).unwrap();
        let (session, commands) = (None, 0);
        Self {
            f,
            home,
            git,
            lfs,
            session,
            commands,
        }
    }

    /// Commits the work tree, then starts the session.
    pub(super) async fn start(&mut self) {
        self.git(&["add", "-A"], None).await;
        self.git(&["commit", "-qm", "base"], None).await;
        self.session = Some(self.f.started().await);
    }

    pub(super) fn session(&self) -> &SessionId {
        self.session.as_ref().expect("started")
    }

    pub(super) fn path(&self, path: &str) -> PathBuf {
        self.f.project().join(path)
    }

    pub(super) fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let path = self.path(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    pub(super) fn chmod(&self, path: &str, mode: u32) {
        let mode = fs::Permissions::from_mode(mode);
        fs::set_permissions(self.path(path), mode).unwrap();
    }

    /// The user's `git <args>` in the project, `input` on stdin; its stdout.
    pub(super) async fn git(&self, args: &[&str], input: Option<&[u8]>) -> Vec<u8> {
        let mut env = Env::new();
        env.set("HOME", self.home.path()).unwrap();
        env.set("GIT_CONFIG_NOSYSTEM", "1").unwrap();
        if let Some(lfs) = &self.lfs {
            let path = [lfs.parent().unwrap(), self.git.parent().unwrap()];
            env.set("PATH", std::env::join_paths(path).unwrap())
                .unwrap();
        }
        let mut git = Command::new(&self.git);
        let who = ["-c", "user.name=t", "-c", "user.email=t@example.invalid"];
        git.args(who)
            .args(args)
            .current_dir(self.f.project())
            .env(env);
        let input = input.map(<[u8]>::to_vec);
        let out = run(&ManualClock::new(), git, input, Duration::from_secs(30)).await;
        let out = out.unwrap();
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(out.status.success(), "git {args:?}: {err}");
        out.stdout
    }

    /// The index's blob and mode of `path`.
    async fn index(&self, path: &str) -> Held {
        let args = ["--literal-pathspecs", "ls-files", "-s", "--", path];
        let entry = String::from_utf8(self.git(&args, None).await).unwrap();
        let mut fields = entry.split([' ', '\t']);
        let (mode, oid) = (fields.next()?, fields.next()?);
        held(self.git(&["cat-file", "blob", oid], None).await, mode)
    }

    /// The work tree's bytes and mode of `path`; a symbolic link's target.
    fn tree(&self, path: &str) -> Held {
        let path = self.path(path);
        let meta = fs::symlink_metadata(&path).ok()?;
        if meta.file_type().is_symlink() {
            let target = fs::read_link(&path).unwrap().into_os_string();
            return held(target.into_encoded_bytes(), "120000");
        }
        let exec = meta.permissions().mode() & 0o100 != 0;
        let mode = if exec { "100755" } else { "100644" };
        held(fs::read(&path).unwrap(), mode)
    }

    /// A diff of `scope` made now: its token, and the units of `path`.
    async fn units(&self, scope: Value, path: &str) -> (Value, Vec<Value>) {
        let diff = json!({"session_id": self.session(), "scope": scope});
        let token = self.f.ok("git/diff", diff).await["diff_token"].take();
        let file = json!({"session_id": self.session(), "diff_token": token, "path": path});
        let units = self.f.ok("git/file_diff", file).await["hunks"].take();
        (token, units.as_array().unwrap().clone())
    }

    /// The token of `scope` and the ID of `path`'s unit `pick`.
    async fn unit(&self, scope: Value, path: &str, pick: Pick) -> (Value, Value) {
        let (token, units) = self.units(scope, path).await;
        (token, id(&units, pick))
    }

    /// Each step in turn, from a diff made now of the range its operation
    /// takes: `staged` to unstage, else `unstaged`.
    async fn steps(&mut self, steps: Vec<Step>) {
        for (label, op, path, pick, want) in steps {
            let scope = if op == "unstage" {
                "staged"
            } else {
                "unstaged"
            };
            let (token, id) = self.unit(json!(scope), path, pick).await;
            let done = self.apply(op, &token, &[id], path).await;
            assert!(done.is_ok(), "{label} ({path:?}): {done:?}");
            let got = match op {
                "revert" => self.tree(path),
                _ => self.index(path).await,
            };
            assert_eq!(got, want, "{label} ({path:?})");
        }
    }

    /// `op` of the units `ids` of the diff `token`. A staging leaves `path`
    /// in the work tree as it was, a revert the index's entries.
    async fn apply(&mut self, op: &str, token: &Value, ids: &[Value], path: &str) -> Done {
        self.commands += 1;
        let params = json!({"command_id": format!("h-{}", self.commands),
            "session_id": self.session(), "diff_token": token, "target": {"hunk_ids": ids}});
        let (entries, tree) = (self.git(&["ls-files", "-s"], None).await, self.tree(path));
        let done = self.f.call(harness(), &format!("git/{op}"), params).await;
        match op {
            "revert" => assert_eq!(self.git(&["ls-files", "-s"], None).await, entries, "{path}"),
            _ => assert_eq!(self.tree(path), tree, "{op} left the work tree"),
        }
        done
    }

    /// The code and reason `op` of `ids` is refused with.
    async fn refused(&mut self, op: &str, token: &Value, ids: &[Value], path: &str) -> String {
        code(self.apply(op, token, ids, path).await.unwrap_err())
    }
}

type Done = Result<Value, Value>;

/// The ID of the unit `pick` of `units`.
fn id(units: &[Value], pick: Pick) -> Value {
    let changed = |unit: &&Value, marker: &str| {
        let lines = unit["lines"].as_array().into_iter().flatten();
        let mut lines = lines.filter_map(Value::as_str);
        lines.any(|line| line.starts_with(['+', '-']) && line.contains(marker))
    };
    let found = units.iter().find(|unit| match pick {
        First => true,
        Mode => unit["unit"] == "mode",
        Marked(marker) => changed(unit, marker),
    });
    found.unwrap_or_else(|| panic!("no unit {pick:?} in {units:?}"))["hunk_id"].clone()
}

/// The kinds of `units`.
fn kinds(units: &[Value]) -> Vec<&str> {
    let kinds = units.iter().filter_map(|unit| unit["unit"].as_str());
    kinds.collect()
}

/// SP-5's `text_multi_hunk`: three hunks in a file whose lines end with `eol`
/// in the work tree and `index` in the index; `config` and `attributes`
/// before the commit.
async fn multi_hunk(name: &str, eol: &str, index: &str, config: &[&str], attributes: &str) {
    let mut c = Case::new(name).await;
    if !config.is_empty() {
        c.git(config, None).await;
    }
    if !attributes.is_empty() {
        c.write(".gitattributes", attributes);
    }
    let (base, base_index) = (lines(30, eol), lines(30, index));
    c.write("f.txt", base.concat());
    c.start().await;
    if !config.is_empty() || !attributes.is_empty() {
        // Out again as the configuration has it.
        fs::remove_file(c.path("f.txt")).unwrap();
        c.git(&["checkout", "--", "f.txt"], None).await;
    }
    c.write("f.txt", edit(&base, &[A, B, C], eol));
    let staged = |edits: &[E]| text(edit(&base_index, edits, index));
    let reverted = text(edit(&base, &[B, C], eol));
    steps![c;
        "stage hunk B (middle)": stage "f.txt", Marked("EDIT-B") => staged(&[B]);
        "stage hunk C (after B staged)": stage "f.txt", Marked("line 27") => staged(&[B, C]);
        "unstage hunk C": unstage "f.txt", Marked("line 27") => staged(&[B]);
        "revert hunk A (worktree)": revert "f.txt", Marked("EDIT-A") => reverted;
    ];
}

#[tokio::test]
async fn it_git_hunk_multi_hunk() {
    multi_hunk("hunk-multi", "\n", "\n", &[], "").await;
}

#[tokio::test]
async fn it_git_hunk_crlf_raw() {
    multi_hunk("hunk-crlf-raw", "\r\n", "\r\n", &[], "").await;
}

#[tokio::test]
async fn it_git_hunk_crlf_eol_attr() {
    multi_hunk("hunk-crlf-attr", "\r\n", "\n", &[], "*.txt text eol=crlf\n").await;
}

#[tokio::test]
async fn it_git_hunk_crlf_autocrlf() {
    let config = ["config", "core.autocrlf", "true"];
    multi_hunk("hunk-crlf-auto", "\r\n", "\n", &config, "").await;
}

#[tokio::test]
async fn it_git_hunk_mixed_eol() {
    let mut c = Case::new("hunk-mixed").await;
    let crlf = |(i, line): (usize, String)| match i % 2 {
        1 => line,
        _ => line.replace('\n', "\r\n"),
    };
    let base: Vec<String> = lines(30, "\n").into_iter().enumerate().map(crlf).collect();
    c.write("m.txt", base.concat());
    c.start().await;
    let edits = [E::R(2, "line 02 EDIT-A\r\n"), E::R(15, "line 15 EDIT-B\n")];
    c.write("m.txt", edit(&base, &edits, "\n"));
    let b = text(edit(&base, &edits[1..], "\n"));
    steps![c;
        "stage hunk B": stage "m.txt", Marked("EDIT-B") => b.clone();
        "revert hunk A": revert "m.txt", Marked("EDIT-A") => b;
    ];
}

#[tokio::test]
async fn it_git_hunk_no_eol() {
    let mut c = Case::new("hunk-no-eol").await;
    let newline = lines(20, "\n").concat();
    let base = &newline[..newline.len() - 1];
    for (path, content) in [("a.txt", base), ("b.txt", base), ("c.txt", &newline)] {
        c.write(path, content);
    }
    c.start().await;
    let edited = base.replace("line 02", "line 02 EDIT-A");
    let z = |text: &str| format!("{}19 EDIT-Z", &text[..text.len() - 2]);
    c.write("a.txt", z(&edited));
    c.write("b.txt", &newline);
    c.write("c.txt", base);
    steps![c;
        "stage last-line hunk (no EOL)": stage "a.txt", Marked("EDIT-Z") => text(z(base));
        "unstage it": unstage "a.txt", Marked("EDIT-Z") => text(base);
        "revert last-line hunk": revert "a.txt", Marked("EDIT-Z") => text(edited);
        "stage 'add trailing newline'": stage "b.txt", First => text(newline.clone());
        "revert 'remove trailing newline'": revert "c.txt", First => text(newline.clone());
    ];
}

#[tokio::test]
async fn it_git_hunk_new_file() {
    let mut c = Case::new("hunk-new").await;
    c.write("keep.txt", "x\n");
    c.start().await;
    let data = lines(12, "\n").concat();
    c.write("n.txt", &data);
    c.git(&["add", "n.txt"], None).await;
    steps![c; "unstage staged new file": unstage "n.txt", First => None;];
    // `unstaged` has it, a whole file: no `diff --no-index` needed.
    let (_, units) = c.units(json!("unstaged"), "n.txt").await;
    let got = kinds(&units);
    assert_eq!(got, ["whole_file"], "untracked file shows in the diff");
    steps![c; "stage untracked file": stage "n.txt", First => text(data);];
    c.git(&["rm", "-q", "--cached", "n.txt"], None).await;
    steps![c; "revert untracked file (apply -R deletes it)": revert "n.txt", First => None;];
    assert_eq!(c.index("n.txt").await, None, "not in the index either");
}

#[tokio::test]
async fn it_git_hunk_empty_file() {
    let mut c = Case::new("hunk-empty").await;
    c.write("t.txt", "one\ntwo\n");
    c.start().await;
    c.write("e.txt", "");
    c.write("t.txt", "");
    let (_, units) = c.units(json!("unstaged"), "e.txt").await;
    let got = (kinds(&units), &units[0]["lines"]);
    let want = (vec!["whole_file"], &json!([]));
    assert_eq!(got, want, "empty new file patch: a unit, no lines");
    steps![c;
        "stage empty untracked file": stage "e.txt", First => text("");
        "stage truncation to empty": stage "t.txt", First => text("");
        "unstage truncation": unstage "t.txt", First => text("one\ntwo\n");
        "revert truncation": revert "t.txt", First => text("one\ntwo\n");
    ];
}

#[tokio::test]
async fn it_git_hunk_deleted_file() {
    let mut c = Case::new("hunk-deleted").await;
    let data = lines(10, "\n").concat();
    c.write("d.sh", &data);
    c.chmod("d.sh", 0o755);
    c.start().await;
    fs::remove_file(c.path("d.sh")).unwrap();
    let exec = held(data, "100755");
    steps![c;
        "stage deletion": stage "d.sh", First => None;
        "unstage deletion": unstage "d.sh", First => exec.clone();
        "revert deletion (recreate file + exec bit)": revert "d.sh", First => exec.clone();
    ];
    assert_eq!(c.index("d.sh").await, exec, "the index keeps it");
}

#[tokio::test]
async fn it_git_hunk_rename() {
    let mut c = Case::new("hunk-rename").await;
    let base = lines(30, "\n");
    c.write("old.txt", base.concat());
    c.start().await;
    c.git(&["mv", "old.txt", "new.txt"], None).await;
    let new = edit(&base, &[A, C], "\n");
    c.write("new.txt", &new);
    c.git(&["add", "new.txt"], None).await;
    let staged = json!({"session_id": c.session(), "scope": "staged"});
    let renamed = c.f.ok("git/diff", staged).await["files"][0]["old_path"].take();
    assert_eq!(renamed, "old.txt", "staged diff shows rename");
    // Its units: the source's deletion and the addition, whole files.
    let (token, units) = c.units(json!("staged"), "new.txt").await;
    assert_eq!(kinds(&units), ["whole_file"; 2], "--no-renames staged diff");
    let added = [id(&units, Marked("EDIT-A"))];
    c.apply("unstage", &token, &added, "new.txt").await.unwrap();
    let index = (c.index("new.txt").await, c.index("old.txt").await);
    let got = (index, c.tree("new.txt"));
    assert_eq!(got, ((None, None), text(new)), "unstage the add side only");
    let again = c.refused("unstage", &token, &added, "new.txt").await;
    assert_eq!(again, "conflict stale_diff", "the same unit again");
    // A plain `mv`: the snapshot has the new path, so `unstaged` shows the rename.
    c.git(&["reset", "-q", "--hard"], None).await;
    c.git(&["clean", "-fdq"], None).await;
    fs::rename(c.path("old.txt"), c.path("moved.txt")).unwrap();
    let unstaged = json!({"session_id": c.session(), "scope": "unstaged"});
    let file = c.f.ok("git/diff", unstaged).await["files"][0].take();
    let moved = (&file["path"], &file["old_path"]);
    let want = (&json!("moved.txt"), &json!("old.txt"));
    assert_eq!(moved, want, "unstaged view of a plain mv");
}

#[tokio::test]
async fn it_git_hunk_binary() {
    let mut c = Case::new("hunk-binary").await;
    let b0: Vec<u8> = (0..8).flat_map(|_| 0..=255u8).collect();
    let b1 = [&b0[..500], b"\0CHANGED\0", &b0[520..]].concat();
    c.write("img.bin", &b0);
    c.start().await;
    c.write("img.bin", &b1);
    let (_, units) = c.units(json!("unstaged"), "img.bin").await;
    let header = &units[0]["header"];
    assert_eq!(header, "GIT binary patch", "binary patch present");
    steps![c;
        "stage binary (--binary patch)": stage "img.bin", First => text(b1);
        "unstage binary": unstage "img.bin", First => text(b0.clone());
        "revert binary": revert "img.bin", First => text(b0);
    ];
}

#[tokio::test]
async fn it_git_hunk_mode_change() {
    let mut c = Case::new("hunk-mode").await;
    let base = lines(30, "\n");
    c.write("s.sh", base.concat());
    c.write("only.sh", "echo\n");
    c.start().await;
    let (a, full) = (edit(&base, &[A], "\n"), edit(&base, &[A, C], "\n"));
    c.write("s.sh", &full);
    c.chmod("s.sh", 0o755);
    c.chmod("only.sh", 0o755);
    let (_, units) = c.units(json!("unstaged"), "s.sh").await;
    let got = kinds(&units);
    assert_eq!(got, ["mode", "hunk", "hunk"], "units offered for s.sh");
    let exec = |text: &[u8]| held(text, "100755");
    let echo = |mode| held("echo\n", mode);
    steps![c;
        "stage hunk A (mode lines stripped)": stage "s.sh", Marked("EDIT-A") => text(a.clone());
        "stage mode pseudo-hunk": stage "s.sh", Mode => exec(&a);
        "stage hunk C": stage "s.sh", Marked("line 27") => exec(&full);
        "stage mode-only change": stage "only.sh", Mode => echo("100755");
        "unstage mode-only change": unstage "only.sh", Mode => echo("100644");
        "revert mode-only change (chmod -x)": revert "only.sh", Mode => echo("100644");
    ];
    // SP-5's "[keep mode lines]": two units of one diff at once, then a
    // third from the same diff, stale by then.
    c.git(&["reset", "-q"], None).await;
    let (token, units) = c.units(json!("unstaged"), "s.sh").await;
    let first = [id(&units, Mode), id(&units, Marked("EDIT-A"))];
    c.apply("stage", &token, &first, "s.sh").await.unwrap();
    assert_eq!(c.index("s.sh").await, exec(&a), "stage the mode and hunk A");
    let third = [id(&units, Marked("line 27"))];
    c.apply("stage", &token, &third, "s.sh").await.unwrap();
    let (got, want) = (c.index("s.sh").await, exec(&full));
    assert_eq!(got, want, "stage hunk C from the same diff");
}

#[tokio::test]
async fn it_git_hunk_symlink() {
    let mut c = Case::new("hunk-symlink").await;
    symlink("target-a", c.path("ln")).unwrap();
    c.start().await;
    fs::remove_file(c.path("ln")).unwrap();
    symlink("target-b", c.path("ln")).unwrap();
    let link = |target| held(target, "120000");
    steps![c;
        "stage symlink retarget": stage "ln", First => link("target-b");
        "unstage symlink retarget": unstage "ln", First => link("target-a");
        "revert symlink retarget": revert "ln", First => link("target-a");
    ];
}

#[tokio::test]
async fn it_git_hunk_special_names() {
    let mut c = Case::new("hunk-names").await;
    let names = [
        "dir with space/file name.txt",
        "unicode/文件-名.txt",
        "quote/a\"b.txt",
        "tab/a\tb.txt",
    ];
    let base = lines(10, "\n");
    for name in names {
        c.write(name, base.concat());
    }
    c.start().await;
    for name in names {
        c.write(name, edit(&base, &[A], "\n"));
    }
    let (old, new) = (text(base.concat()), text(edit(&base, &[A], "\n")));
    for name in names {
        steps![c;
            "stage": stage name, First => new.clone();
            "unstage": unstage name, First => old.clone();
            "revert": revert name, First => old.clone();
        ];
    }
}

#[tokio::test]
async fn it_git_hunk_conflict() {
    let mut c = Case::new("hunk-conflict").await;
    let base = lines(30, "\n");
    c.write("x.txt", base.concat());
    c.write("y.txt", base.concat());
    c.start().await;
    let full = edit(&base, &[A, B, C], "\n");
    c.write("x.txt", &full);
    let stale = "conflict stale_diff";
    // stage: index changed in the same lines -> refuse.
    let (token, a) = c.unit(json!("unstaged"), "x.txt", Marked("EDIT-A")).await;
    let user = edit(&base, &[E::R(2, "line 02 USER\n")], "\n");
    let blob = c.git(&["hash-object", "-w", "--stdin"], Some(&user)).await;
    let entry = format!("100644,{},x.txt", String::from_utf8(blob).unwrap().trim());
    c.git(&["update-index", "--cacheinfo", &entry], None).await;
    let refused = c.refused("stage", &token, &[a], "x.txt").await;
    assert_eq!(refused, stale, "stage: index changed in the same lines");
    let (got, want) = (c.index("x.txt").await, text(user));
    assert_eq!(got, want, "the index as the user left it");
    c.git(&["reset", "-q", "--", "x.txt"], None).await;
    // revert: worktree changed in the same lines -> refuse.
    let (token, a) = c.unit(json!("unstaged"), "x.txt", Marked("EDIT-A")).await;
    let again = edit(&base, &[E::R(2, "line 02 EDIT-A2\n"), B, C], "\n");
    c.write("x.txt", &again);
    let refused = c.refused("revert", &token, &[a], "x.txt").await;
    assert_eq!(refused, stale, "revert: worktree changed in the same lines");
    assert_eq!(c.tree("x.txt"), text(again), "the work tree untouched");
    // revert: unrelated edits + 3-line offset -> applies.
    c.write("x.txt", &full);
    let (token, b) = c.unit(json!("unstaged"), "x.txt", Marked("EDIT-B")).await;
    let top = ["new top 1\n", "new top 2\n", "new top 3\n"].map(String::from);
    let shifted: Vec<String> = top.into_iter().chain(base.clone()).collect();
    let moved = [
        E::R(5, "line 02 EDIT-A\n"),
        E::I(18, "EDIT-B inserted\n"),
        E::D(30),
    ];
    c.write("x.txt", edit(&shifted, &moved, "\n"));
    c.apply("revert", &token, &[b], "x.txt").await.unwrap();
    let want = text(edit(&shifted, &[moved[0], moved[2]], "\n"));
    let got = c.tree("x.txt");
    assert_eq!(got, want, "revert: unrelated edits + 3-line offset");
    // revert 2-file patch, 2nd conflicts -> atomic, x untouched.
    c.write("x.txt", &full);
    c.write("y.txt", &full);
    let (token, x) = c.units(json!("unstaged"), "x.txt").await;
    let file = json!({"session_id": c.session(), "diff_token": token, "path": "y.txt"});
    let y = c.f.ok("git/file_diff", file).await["hunks"].take();
    let y = id(y.as_array().unwrap(), Marked("EDIT-A"));
    let other = edit(&base, &[E::R(2, "line 02 OTHER\n")], "\n");
    c.write("y.txt", &other);
    let ids = [id(&x, Marked("EDIT-A")), y];
    let refused = c.refused("revert", &token, &ids, "x.txt").await;
    assert_eq!(refused, stale, "revert 2-file patch, 2nd conflicts");
    let trees = (c.tree("x.txt"), c.tree("y.txt"));
    assert_eq!(trees, (text(full), text(other)), "atomic, x untouched");
}

#[tokio::test]
async fn it_git_hunk_whitespace_cfg() {
    let mut c = Case::new("hunk-whitespace").await;
    c.git(&["config", "apply.whitespace", "fix"], None).await;
    let base = lines(10, "\n");
    c.write("w.txt", base.concat());
    c.start().await;
    let new = edit(&base, &[E::R(2, "line 02 trailing   \n")], "\n");
    c.write("w.txt", &new);
    // The host passes `--whitespace=nowarn`: the user's `fix` changes nothing.
    steps![c; "stage with --whitespace=nowarn": stage "w.txt", First => text(new);];
}

#[tokio::test]
async fn it_git_hunk_turn_revert() {
    let mut c = Case::new("hunk-turn").await;
    let base = lines(30, "\n");
    c.write("t.txt", base.concat());
    c.start().await;
    // The turn's changes, between its two snapshots.
    c.write("t.txt", edit(&base, &[A, B], "\n"));
    c.write("agent-new.txt", "created by agent\n");
    c.f.emit(done(PromptTag(1), StopReason::EndTurn));
    c.f.events(c.session(), "turn.completed", 1).await;
    let turn = json!({"turn": c.f.rows("SELECT turn_id FROM turns").remove(0)});
    let (token, units) = c.units(turn.clone(), "agent-new.txt").await;
    let created = [id(&units, First)];
    let got = kinds(&units);
    assert_eq!(got, ["whole_file"], "turn diff includes the new file");
    let later = E::R(25, "line 25 USER-LATER\n");
    c.write("t.txt", edit(&base, &[A, B, later], "\n"));
    let (turned, a) = c.unit(turn.clone(), "t.txt", Marked("EDIT-A")).await;
    c.apply("revert", &turned, &[a], "t.txt").await.unwrap();
    let want = text(edit(&base, &[B, later], "\n"));
    let got = c.tree("t.txt");
    assert_eq!(got, want, "revert turn hunk A after unrelated user edit");
    let path = "agent-new.txt";
    c.apply("revert", &token, &created, path).await.unwrap();
    assert_eq!(c.tree(path), None, "revert turn-created untracked file");
    let rewritten = edit(&base, &[A, E::R(16, "EDIT-B changed by user\n")], "\n");
    c.write("t.txt", &rewritten);
    let (turned, b) = c.unit(turn, "t.txt", Marked("EDIT-B")).await;
    let refused = c.refused("revert", &turned, &[b], "t.txt").await;
    let stale = "conflict stale_diff";
    assert_eq!(refused, stale, "revert turn hunk B after a rewrite");
    let (got, want) = (c.tree("t.txt"), text(rewritten));
    assert_eq!(got, want, "the user's rewrite untouched");
}

#[tokio::test]
async fn it_git_hunk_index_lock() {
    let mut c = Case::new("hunk-lock").await;
    let base = lines(30, "\n");
    c.write("k.txt", base.concat());
    c.start().await;
    c.write("k.txt", edit(&base, &[A, B, C], "\n"));
    let (stage, b) = c.unit(json!("unstaged"), "k.txt", Marked("EDIT-B")).await;
    let (revert, a) = c.unit(json!("unstaged"), "k.txt", Marked("EDIT-A")).await;
    // The user's git is running.
    c.write(".git/index.lock", "");
    let refused = c.refused("stage", &stage, &[b], "k.txt").await;
    let held_lock = "git_error lock_held";
    assert_eq!(refused, held_lock, "stage while index.lock is held");
    let (got, want) = (c.index("k.txt").await, text(base.concat()));
    assert_eq!(got, want, "nothing changed");
    c.apply("revert", &revert, &[a], "k.txt").await.unwrap();
    let want = text(edit(&base, &[B, C], "\n"));
    let got = c.tree("k.txt");
    assert_eq!(got, want, "revert (worktree only) while index.lock is held");
    fs::remove_file(c.path(".git/index.lock")).unwrap();
}

/// T121's hand-off: with the work tree as the index has it, git would apply a
/// staged diff's patch to the work tree in reverse; only the range check
/// stops `git/revert` from taking it.
#[tokio::test]
async fn it_git_hunk_revert_takes_no_staged_diff() {
    let mut c = Case::new("hunk-staged-revert").await;
    let base = lines(10, "\n");
    c.write("f.txt", base.concat());
    c.start().await;
    let a = edit(&base, &[A], "\n");
    c.write("f.txt", &a);
    c.git(&["add", "f.txt"], None).await;
    let (token, id) = c.unit(json!("staged"), "f.txt", First).await;
    let refused = c.refused("revert", &token, &[id], "f.txt").await;
    assert_eq!(refused, "conflict stale_diff");
    assert_eq!(c.tree("f.txt"), text(a), "the work tree untouched");
}
