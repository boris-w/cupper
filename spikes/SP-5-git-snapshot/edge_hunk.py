"""Correctness of stage / unstage / revert for single hunks, on small repos.

Every case builds a fresh repo, applies independent edits whose expected result
is computed directly from the edit list (not from git), then checks the index
blob, index mode, worktree bytes and worktree mode after each operation.

Patches always come from the git CLI (`git diff`, pinned options in hunks.py).
They are applied by a backend:
  git-cli  stage = apply --cached ; unstage = apply --cached -R ; revert = apply -R
  libgit2  (pygit2) git_apply to INDEX / WORKDIR; libgit2 has no "reverse", so
           unstage/revert use a patch generated with `git diff -R`.

Usage:
  python3 edge_hunk.py                       # git-cli backend
  uv run --with pygit2 python edge_hunk.py --backend libgit2
"""
from __future__ import annotations

import argparse
import os
import stat
import sys
from pathlib import Path
from typing import Callable, List, Optional

import hunks
from lib import (GitError, SCRATCH, STATE, commit_all, file_state, git, git_out, git_path, md_table, new_repo,
                 save_result)
from snapshot import snapshot

# --------------------------------------------------------------------------- backends


class CliBackend:
    name = "git-cli"
    reversed_diff = False

    def apply(self, repo, kind: str, patch: bytes, raw: bool = False) -> None:
        args = {"stage": ["--cached"], "unstage": ["--cached", "-R"], "revert": ["-R"]}[kind]
        if raw:  # without our pinned --whitespace=nowarn
            git(repo, "apply", *args, "-", input=patch)
        else:
            hunks.apply(repo, patch, *args)


class Libgit2Backend:
    name = "libgit2"
    reversed_diff = True

    def __init__(self, fix_header: bool = False):
        self.fix_header = fix_header
        if fix_header:
            self.name = "libgit2+fix"
        import pygit2  # noqa: F401

        self.pygit2 = pygit2
        empty = STATE / "libgit2-empty-config"
        empty.mkdir(parents=True, exist_ok=True)
        for lvl in ("GIT_CONFIG_LEVEL_SYSTEM", "GIT_CONFIG_LEVEL_GLOBAL", "GIT_CONFIG_LEVEL_XDG"):
            level = getattr(pygit2, lvl, None)
            if level is None:
                level = getattr(pygit2.enums.ConfigLevel, lvl.replace("GIT_CONFIG_LEVEL_", ""))
            pygit2.settings.search_path[level] = str(empty)
        loc = getattr(pygit2, "enums", None)
        self.INDEX = loc.ApplyLocation.INDEX if loc else pygit2.GIT_APPLY_LOCATION_INDEX
        self.WORKDIR = loc.ApplyLocation.WORKDIR if loc else pygit2.GIT_APPLY_LOCATION_WORKDIR
        self.version = f"pygit2 {pygit2.__version__} / libgit2 {pygit2.LIBGIT2_VERSION}"

    def apply(self, repo, kind: str, patch: bytes, raw: bool = False) -> None:
        r = self.pygit2.Repository(str(repo))
        if self.fix_header:
            patch = hunks.normalize_new_start(patch)
        try:
            d = self.pygit2.Diff.parse_diff(patch)
            r.apply(d, self.INDEX if kind in ("stage", "unstage") else self.WORKDIR)
        except Exception as e:  # normalise to GitError-like failure
            raise GitError(["libgit2", "apply", kind], _Fake(str(e)))


class _Fake:
    def __init__(self, msg):
        self.returncode = 1
        self.stderr = msg.encode()


# --------------------------------------------------------------------------- helpers


def L(n: int, eol: bytes = b"\n", prefix: str = "line") -> List[bytes]:
    return [f"{prefix} {i:02d}".encode() + eol for i in range(n)]


def edit(lines: List[bytes], edits) -> bytes:
    """edits: list of (op, idx, payload). Applied bottom-up so indexes stay valid."""
    out = list(lines)
    for op, idx, payload in sorted(edits, key=lambda e: -e[1]):
        if op == "replace":
            out[idx] = payload
        elif op == "insert":  # after idx
            out[idx + 1:idx + 1] = payload
        elif op == "delete":
            del out[idx]
    return b"".join(out)


def index_blob(repo, path: str) -> Optional[bytes]:
    p = git(repo, "cat-file", "blob", f":{path}", check=False)
    return p.stdout if p.returncode == 0 else None


def index_mode(repo, path: str) -> Optional[str]:
    out = git_out(repo, "ls-files", "-s", "--", path)
    return out.split()[0] if out else None


def wt_mode(p: Path) -> Optional[str]:
    if not os.path.lexists(p):
        return None
    st = os.lstat(p)
    if stat.S_ISLNK(st.st_mode):
        return "120000"
    return "100755" if st.st_mode & stat.S_IXUSR else "100644"


def pick(files, marker: bytes, keep_mode: bool = False) -> bytes:
    for fp in files:
        for i, h in enumerate(fp.hunks):
            if any(l[:1] in (b"+", b"-") and marker in l for l in h.lines):
                return fp.hunk_patch(i, keep_mode=keep_mode)
    raise LookupError(f"no hunk with {marker!r}")


def whole(files, path_marker: bytes) -> bytes:
    for fp in files:
        if path_marker in fp.header[0]:
            return fp.whole()
    raise LookupError(path_marker)


class Ctx:
    def __init__(self, backend, case: str):
        self.b = backend
        self.case = case
        self.repo = new_repo(f"hunk-{backend.name}-{case}")
        self.rows: List[dict] = []

    # patch for an op, optionally restricted to paths
    def patch(self, kind: str, *paths: str, extra=()) -> list:
        args = []
        if kind == "unstage":
            args.append("--cached")
        if self.b.reversed_diff and kind in ("unstage", "revert"):
            args.append("-R")
        args += list(extra)
        if paths:
            args += ["--", *paths]
        return hunks.parse(hunks.diff(self.repo, *args))

    def op(self, label: str, kind: str, patch: bytes, expect: Callable[[], List[tuple]],
           expect_fail: bool = False, raw: bool = False) -> bool:
        idx_before = file_state(git_path(self.repo, "index"))
        entries_before = git_out(self.repo, "ls-files", "-s")
        err = ""
        try:
            self.b.apply(self.repo, kind, patch, raw=raw)
            failed = False
        except GitError as e:
            failed, err = True, e.stderr.strip().splitlines()[-1] if e.stderr.strip() else str(e)
        checks = expect()
        bad = [c for c in checks if c[1] != c[2]]
        detail = []
        if kind == "revert":  # a worktree revert must never change index entries
            if git_out(self.repo, "ls-files", "-s") != entries_before:
                bad.append(("index entries changed by revert", "unchanged", "changed"))
            elif file_state(git_path(self.repo, "index")) != idx_before:
                detail.append("note: index file rewritten (stat refresh, entries unchanged)")
        ok = (failed == expect_fail) and not bad
        if failed:
            detail.append(("expected failure: " if expect_fail else "FAILED: ") + err[:160])
        elif expect_fail:
            detail.append("UNEXPECTEDLY APPLIED")
        for what, want, got in bad:
            detail.append(f"{what}: want {short(want)} got {short(got)}")
        self.rows.append({"case": self.case, "op": label, "backend": self.b.name, "ok": ok,
                          "detail": "; ".join(detail)})
        return ok

    def note(self, label: str, ok: bool, detail: str) -> None:
        self.rows.append({"case": self.case, "op": label, "backend": self.b.name, "ok": ok, "detail": detail})


def short(v) -> str:
    if isinstance(v, bytes):
        s = repr(v[:60]) + ("…" if len(v) > 60 else "")
        return s
    return repr(v)


def chk_index(c: Ctx, path: str, want: Optional[bytes], mode: Optional[str] = None) -> List[tuple]:
    out = [(f"index[{path}]", want, index_blob(c.repo, path))]
    if mode:
        out.append((f"index mode[{path}]", mode, index_mode(c.repo, path)))
    return out


def chk_wt(c: Ctx, path: str, want: Optional[bytes], mode: Optional[str] = None) -> List[tuple]:
    p = c.repo / path
    if want is None:
        return [(f"worktree[{path}] exists", False, os.path.lexists(p))]
    got = os.readlink(p).encode() if os.path.islink(p) else (p.read_bytes() if p.exists() else None)
    out = [(f"worktree[{path}]", want, got)]
    if mode:
        out.append((f"worktree mode[{path}]", mode, wt_mode(p)))
    return out


# --------------------------------------------------------------------------- cases

EDITS = [("replace", 2, b"line 02 EDIT-A\n"), ("insert", 15, [b"EDIT-B inserted\n"]), ("delete", 27, None)]


def text_multi_hunk(c: Ctx, eol: bytes = b"\n", fname: str = "f.txt", attrs: Optional[bytes] = None,
                    cfg: Optional[tuple] = None, index_eol: Optional[bytes] = None) -> None:
    """3 hunks: replace line 2, insert after 15, delete 27. Stage B, stage C, unstage C, revert A."""
    repo = c.repo
    if cfg:
        git(repo, "config", *cfg)
    if attrs:
        (repo / ".gitattributes").write_bytes(attrs)
    ie = index_eol if index_eol is not None else eol
    edits = [(op, i, (p.replace(b"\n", eol) if isinstance(p, bytes) else
                      [x.replace(b"\n", eol) for x in p] if p else p)) for op, i, p in EDITS]
    iedits = [(op, i, (p.replace(b"\n", ie) if isinstance(p, bytes) else
                       [x.replace(b"\n", ie) for x in p] if p else p)) for op, i, p in EDITS]
    base_w, base_i = L(30, eol), L(30, ie)
    (repo / fname).write_bytes(b"".join(base_w))
    commit_all(repo)
    if attrs or cfg:  # re-checkout so the worktree uses the configured EOL
        (repo / fname).unlink()
        git(repo, "checkout", "--", fname)
    full_w = edit(base_w, edits)
    (repo / fname).write_bytes(full_w)

    def I(*ks):
        return edit(base_i, [iedits[k] for k in ks])

    def W(*ks):
        return edit(base_w, [edits[k] for k in ks])

    c.op("stage hunk B (middle)", "stage", pick(c.patch("stage", fname), b"EDIT-B"),
         lambda: chk_index(c, fname, I(1)) + chk_wt(c, fname, full_w))
    c.op("stage hunk C (after B staged)", "stage", pick(c.patch("stage", fname), b"line 27"),
         lambda: chk_index(c, fname, I(1, 2)) + chk_wt(c, fname, full_w))
    c.op("unstage hunk C", "unstage", pick(c.patch("unstage", fname), b"line 27"),
         lambda: chk_index(c, fname, I(1)) + chk_wt(c, fname, full_w))
    c.op("revert hunk A (worktree)", "revert", pick(c.patch("revert", fname), b"EDIT-A"),
         lambda: chk_index(c, fname, I(1)) + chk_wt(c, fname, W(1, 2)))


def case_multi_hunk(c):
    text_multi_hunk(c)


def case_crlf_raw(c):
    """File committed with CRLF bytes, no attributes, no autocrlf."""
    text_multi_hunk(c, eol=b"\r\n")


def case_crlf_eol_attr(c):
    """.gitattributes text eol=crlf: index LF, worktree CRLF."""
    text_multi_hunk(c, eol=b"\r\n", attrs=b"*.txt text eol=crlf\n", index_eol=b"\n")


def case_crlf_autocrlf(c):
    """core.autocrlf=true: index LF, worktree CRLF."""
    text_multi_hunk(c, eol=b"\r\n", cfg=("core.autocrlf", "true"), index_eol=b"\n")


def case_mixed_eol(c):
    """A file with some CRLF and some LF lines, no conversion."""
    repo = c.repo
    base = [l if i % 2 else l.replace(b"\n", b"\r\n") for i, l in enumerate(L(30))]
    (repo / "m.txt").write_bytes(b"".join(base))
    commit_all(repo)
    edits = [("replace", 2, b"line 02 EDIT-A\r\n"), ("replace", 15, b"line 15 EDIT-B\n")]
    (repo / "m.txt").write_bytes(edit(base, edits))
    c.op("stage hunk B", "stage", pick(c.patch("stage", "m.txt"), b"EDIT-B"),
         lambda: chk_index(c, "m.txt", edit(base, edits[1:])))
    c.op("revert hunk A", "revert", pick(c.patch("revert", "m.txt"), b"EDIT-A"),
         lambda: chk_wt(c, "m.txt", edit(base, edits[1:])))


def case_no_eol(c):
    """Files without a trailing newline: change last line, add newline, remove newline."""
    repo = c.repo
    base = b"".join(L(20))[:-1]  # last line has no \n
    (repo / "a.txt").write_bytes(base)
    (repo / "b.txt").write_bytes(base)
    (repo / "c.txt").write_bytes(base + b"\n")
    commit_all(repo)
    a_new = base.replace(b"line 02", b"line 02 EDIT-A")[:-2] + b"19 EDIT-Z"  # 2 hunks, last w/o newline
    b_new = base + b"\n"           # add the missing newline
    c_new = base                    # drop the trailing newline
    (repo / "a.txt").write_bytes(a_new)
    (repo / "b.txt").write_bytes(b_new)
    (repo / "c.txt").write_bytes(c_new)
    a_half = base[:-2] + b"19 EDIT-Z"
    c.op("stage last-line hunk (no EOL)", "stage", pick(c.patch("stage", "a.txt"), b"EDIT-Z"),
         lambda: chk_index(c, "a.txt", a_half))
    c.op("unstage it", "unstage", pick(c.patch("unstage", "a.txt"), b"EDIT-Z"),
         lambda: chk_index(c, "a.txt", base))
    c.op("revert last-line hunk", "revert", pick(c.patch("revert", "a.txt"), b"EDIT-Z"),
         lambda: chk_wt(c, "a.txt", base.replace(b"line 02", b"line 02 EDIT-A")))
    c.op("stage 'add trailing newline'", "stage", c.patch("stage", "b.txt")[0].hunk_patch(0),
         lambda: chk_index(c, "b.txt", b_new))
    c.op("revert 'remove trailing newline'", "revert", c.patch("revert", "c.txt")[0].hunk_patch(0),
         lambda: chk_wt(c, "c.txt", base + b"\n"))


def case_new_file(c):
    """Staged new file: unstage it; untracked file: stage via `diff --no-index` patch; revert = delete."""
    repo = c.repo
    (repo / "keep.txt").write_bytes(b"x\n")
    commit_all(repo)
    data = b"".join(L(12))
    (repo / "n.txt").write_bytes(data)
    git(repo, "add", "n.txt")
    c.op("unstage staged new file", "unstage", c.patch("unstage", "n.txt")[0].hunk_patch(0),
         lambda: chk_index(c, "n.txt", None) + chk_wt(c, "n.txt", data))
    # untracked: no `git diff` output at all -> build the patch with --no-index
    p = git(repo, "diff", "--no-index", *hunks.DIFF_OPTS, "--", "/dev/null", "n.txt", check=False).stdout
    files = hunks.parse(p)
    c.note("untracked file shows in `git diff`?", True,
           "no (expected): untracked files need `git diff --no-index /dev/null <path>` or status")
    c.op("stage untracked file via --no-index patch", "stage", files[0].whole(),
         lambda: chk_index(c, "n.txt", data, "100644") + chk_wt(c, "n.txt", data))
    git(repo, "rm", "-q", "--cached", "n.txt")
    if c.b.reversed_diff:
        p = git(repo, "diff", "--no-index", *hunks.DIFF_OPTS, "--", "n.txt", "/dev/null", check=False).stdout
        rp = hunks.parse(p)[0].whole()
    else:
        rp = files[0].whole()
    c.op("revert untracked file (apply -R deletes it)", "revert", rp,
         lambda: chk_wt(c, "n.txt", None) + chk_index(c, "n.txt", None))


def case_empty_file(c):
    repo = c.repo
    (repo / "t.txt").write_bytes(b"one\ntwo\n")
    commit_all(repo)
    (repo / "e.txt").write_bytes(b"")
    (repo / "t.txt").write_bytes(b"")
    p = git(repo, "diff", "--no-index", *hunks.DIFF_OPTS, "--", "/dev/null", "e.txt", check=False).stdout
    files = hunks.parse(p)
    c.note("empty new file patch has hunks?", True, f"hunks={len(files[0].hunks) if files else 'no patch'}")
    if files:
        c.op("stage empty untracked file", "stage", files[0].whole(), lambda: chk_index(c, "e.txt", b""))
    c.op("stage truncation to empty", "stage", c.patch("stage", "t.txt")[0].hunk_patch(0),
         lambda: chk_index(c, "t.txt", b""))
    c.op("unstage truncation", "unstage", c.patch("unstage", "t.txt")[0].hunk_patch(0),
         lambda: chk_index(c, "t.txt", b"one\ntwo\n"))
    c.op("revert truncation", "revert", c.patch("revert", "t.txt")[0].hunk_patch(0),
         lambda: chk_wt(c, "t.txt", b"one\ntwo\n"))


def case_deleted_file(c):
    repo = c.repo
    data = b"".join(L(10))
    (repo / "d.sh").write_bytes(data)
    os.chmod(repo / "d.sh", 0o755)
    commit_all(repo)
    (repo / "d.sh").unlink()
    c.op("stage deletion", "stage", c.patch("stage", "d.sh")[0].whole(),
         lambda: chk_index(c, "d.sh", None) + chk_wt(c, "d.sh", None))
    c.op("unstage deletion", "unstage", c.patch("unstage", "d.sh")[0].whole(),
         lambda: chk_index(c, "d.sh", data, "100755"))
    c.op("revert deletion (recreate file + exec bit)", "revert", c.patch("revert", "d.sh")[0].whole(),
         lambda: chk_wt(c, "d.sh", data, "100755") + chk_index(c, "d.sh", data))


def case_rename(c):
    """Staged rename with 2 content hunks. Unstage hunk-by-hunk with -M patches vs --no-renames patches."""
    repo = c.repo
    base = L(30)
    (repo / "old.txt").write_bytes(b"".join(base))
    commit_all(repo)
    git(repo, "mv", "old.txt", "new.txt")
    new = edit(base, [EDITS[0], EDITS[2]])
    (repo / "new.txt").write_bytes(new)
    git(repo, "add", "new.txt")
    files = c.patch("unstage", extra=["-M"])
    c.note("staged diff shows rename", files[0].is_rename and len(files[0].hunks) == 2,
           f"rename={files[0].is_rename} hunks={len(files[0].hunks)}")
    # unstage one hunk of a rename patch. What the user wants: rename and hunk C stay staged.
    c.op("unstage hunk A of a rename (-M patch); want rename+C kept", "unstage", pick(files, b"EDIT-A"),
         lambda: chk_index(c, "new.txt", edit(base, [EDITS[2]])) + chk_index(c, "old.txt", None))
    # second hunk now has no rename target in the index
    try:
        second = pick(files, b"line 27")
        c.op("unstage hunk C of the same rename patch afterwards", "unstage", second, lambda: [],
             expect_fail=True)
    except LookupError as e:
        c.note("unstage hunk C", False, str(e))
    # --no-renames: delete + add as two whole-file units
    git(repo, "reset", "-q")
    git(repo, "add", "-A")
    files = c.patch("unstage", extra=["--no-renames"])
    c.note("--no-renames staged diff", len(files) == 2, " / ".join(
        ("new" if f.is_new else "deleted" if f.is_deleted else "mod") for f in files))
    c.op("unstage the add side only (--no-renames)", "unstage", whole(files, b"new.txt"),
         lambda: chk_index(c, "new.txt", None) + chk_index(c, "old.txt", None) + chk_wt(c, "new.txt", new))
    # worktree-only rename (mv without git): unstaged diff shows only a deletion
    git(repo, "reset", "-q", "--hard")
    os.rename(repo / "old.txt", repo / "moved.txt")
    files = c.patch("stage")
    c.note("unstaged view of a plain `mv`", True,
           f"git diff shows: {[('deleted' if f.is_deleted else 'other') for f in files]}; "
           f"new path is untracked (only visible via status/snapshot)")


def case_binary(c):
    repo = c.repo
    b0 = bytes(range(256)) * 8
    b1 = b0[:500] + b"\x00CHANGED\x00" + b0[520:]
    (repo / "img.bin").write_bytes(b0)
    commit_all(repo)
    (repo / "img.bin").write_bytes(b1)
    files = c.patch("stage", "img.bin")
    c.note("binary patch present", files[0].is_binary, files[0].binary[0][:40].decode(errors="replace").strip())
    c.op("stage binary (--binary patch)", "stage", files[0].whole(),
         lambda: chk_index(c, "img.bin", b1) + chk_wt(c, "img.bin", b1))
    c.op("unstage binary", "unstage", c.patch("unstage", "img.bin")[0].whole(),
         lambda: chk_index(c, "img.bin", b0))
    c.op("revert binary", "revert", c.patch("revert", "img.bin")[0].whole(),
         lambda: chk_wt(c, "img.bin", b0))
    (repo / "img.bin").write_bytes(b1)
    plain = git(repo, "diff", "--no-color", "--", "img.bin").stdout  # no --binary
    c.op("stage binary WITHOUT --binary (expected to fail)", "stage", plain,
         lambda: chk_index(c, "img.bin", b0), expect_fail=True)


def case_mode_change(c):
    repo = c.repo
    base = L(30)
    (repo / "s.sh").write_bytes(b"".join(base))
    (repo / "only.sh").write_bytes(b"echo\n")
    commit_all(repo)
    w = edit(base, [EDITS[0], EDITS[2]])
    (repo / "s.sh").write_bytes(w)
    os.chmod(repo / "s.sh", 0o755)
    os.chmod(repo / "only.sh", 0o755)
    files = c.patch("stage", "s.sh")
    c.note("units offered for s.sh", True, ",".join(files[0].units()))
    c.op("stage hunk A (mode lines stripped)", "stage", pick(files, b"EDIT-A"),
         lambda: chk_index(c, "s.sh", edit(base, [EDITS[0]]), "100644"))
    c.op("stage mode pseudo-hunk", "stage", c.patch("stage", "s.sh")[0].mode_patch(),
         lambda: chk_index(c, "s.sh", edit(base, [EDITS[0]]), "100755"))
    c.op("stage hunk C", "stage", pick(c.patch("stage", "s.sh"), b"line 27"),
         lambda: chk_index(c, "s.sh", w, "100755"))
    # mode-only file
    mp = c.patch("stage", "only.sh")[0]
    c.op("stage mode-only change", "stage", mp.mode_patch(),
         lambda: chk_index(c, "only.sh", b"echo\n", "100755"))
    c.op("unstage mode-only change", "unstage", c.patch("unstage", "only.sh")[0].mode_patch(),
         lambda: chk_index(c, "only.sh", b"echo\n", "100644"))
    c.op("revert mode-only change (chmod -x)", "revert", c.patch("revert", "only.sh")[0].mode_patch(),
         lambda: chk_wt(c, "only.sh", b"echo\n", "100644"))
    # keep_mode=True on every hunk: second hunk carries a stale "old mode"
    git(repo, "reset", "-q")
    files = c.patch("stage", "s.sh")
    pa, pc = pick(files, b"EDIT-A", keep_mode=True), pick(files, b"line 27", keep_mode=True)
    c.op("[keep mode lines] stage hunk A", "stage", pa,
         lambda: chk_index(c, "s.sh", edit(base, [EDITS[0]]), "100755"))
    c.op("[keep mode lines] stage hunk C from the same (stale) diff", "stage", pc,
         lambda: chk_index(c, "s.sh", w, "100755"))


def case_symlink(c):
    repo = c.repo
    os.symlink("target-a", repo / "ln")
    commit_all(repo)
    os.unlink(repo / "ln")
    os.symlink("target-b", repo / "ln")
    c.op("stage symlink retarget", "stage", c.patch("stage", "ln")[0].hunk_patch(0),
         lambda: chk_index(c, "ln", b"target-b", "120000"))
    c.op("unstage symlink retarget", "unstage", c.patch("unstage", "ln")[0].hunk_patch(0),
         lambda: chk_index(c, "ln", b"target-a", "120000"))
    c.op("revert symlink retarget", "revert", c.patch("revert", "ln")[0].hunk_patch(0),
         lambda: chk_wt(c, "ln", b"target-a"))


def case_special_names(c):
    repo = c.repo
    names = ["dir with space/file name.txt", "unicode/文件-名.txt", 'quote/a"b.txt', "tab/a\tb.txt"]
    for n in names:
        (repo / n).parent.mkdir(parents=True, exist_ok=True)
        (repo / n).write_bytes(b"".join(L(10)))
    commit_all(repo)
    for n in names:
        (repo / n).write_bytes(edit(L(10), [EDITS[0]]))
    for n in names:
        c.op(f"stage {n!r}", "stage", c.patch("stage", n)[0].hunk_patch(0),
             lambda n=n: chk_index(c, n, edit(L(10), [EDITS[0]])))
        c.op(f"unstage {n!r}", "unstage", c.patch("unstage", n)[0].hunk_patch(0),
             lambda n=n: chk_index(c, n, b"".join(L(10))))
        c.op(f"revert {n!r}", "revert", c.patch("revert", n)[0].hunk_patch(0),
             lambda n=n: chk_wt(c, n, b"".join(L(10))))


def case_conflict(c):
    repo = c.repo
    base = L(30)
    for f in ("x.txt", "y.txt"):
        (repo / f).write_bytes(b"".join(base))
    commit_all(repo)
    full = edit(base, EDITS)
    (repo / "x.txt").write_bytes(full)
    # 1) stage patch made stale by the index changing underneath (user staged an overlapping change)
    stage_a = pick(c.patch("stage", "x.txt"), b"EDIT-A")
    user = edit(base, [("replace", 2, b"line 02 USER\n")])
    blob = git_out(repo, "hash-object", "-w", "--stdin", input=user)
    git(repo, "update-index", "--cacheinfo", f"100644,{blob},x.txt")
    c.op("stage: index changed in the same lines -> refuse", "stage", stage_a,
         lambda: chk_index(c, "x.txt", user), expect_fail=True)
    git(repo, "reset", "-q", "--", "x.txt")
    # 2) revert patch made stale by the worktree changing in the same lines
    rev_a = pick(c.patch("revert", "x.txt"), b"EDIT-A")
    again = edit(base, [("replace", 2, b"line 02 EDIT-A2\n"), EDITS[1], EDITS[2]])
    (repo / "x.txt").write_bytes(again)
    c.op("revert: worktree changed in the same lines -> refuse", "revert", rev_a,
         lambda: chk_wt(c, "x.txt", again), expect_fail=True)
    # 3) worktree changed elsewhere + lines shifted by an insert at the top -> applies with offset
    (repo / "x.txt").write_bytes(full)
    rev_b = pick(c.patch("revert", "x.txt"), b"EDIT-B")
    shifted = [b"new top 1\n", b"new top 2\n", b"new top 3\n"] + base
    shifted_full = edit(shifted, [("replace", 5, b"line 02 EDIT-A\n"), ("insert", 18, [b"EDIT-B inserted\n"]),
                                  ("delete", 30, None)])
    (repo / "x.txt").write_bytes(shifted_full)
    c.op("revert: unrelated edits + 3-line offset -> applies", "revert", rev_b,
         lambda: chk_wt(c, "x.txt", edit(shifted, [("replace", 5, b"line 02 EDIT-A\n"), ("delete", 30, None)])))
    # 4) two-file patch, second file conflicts -> nothing applied (atomic)
    (repo / "x.txt").write_bytes(full)
    (repo / "y.txt").write_bytes(full)
    files = c.patch("revert", "x.txt", "y.txt")
    two = pick([files[0]], b"EDIT-A") + pick([files[1]], b"EDIT-A")
    y_conflict = edit(base, [("replace", 2, b"line 02 OTHER\n")])
    (repo / "y.txt").write_bytes(y_conflict)
    c.op("revert 2-file patch, 2nd conflicts -> atomic, x untouched", "revert", two,
         lambda: chk_wt(c, "x.txt", full) + chk_wt(c, "y.txt", y_conflict), expect_fail=True)


def case_whitespace_cfg(c):
    """User has apply.whitespace=fix: without --whitespace=nowarn the staged content is altered."""
    repo = c.repo
    git(repo, "config", "apply.whitespace", "fix")
    base = L(10)
    (repo / "w.txt").write_bytes(b"".join(base))
    commit_all(repo)
    new = edit(base, [("replace", 2, b"line 02 trailing   \n")])
    (repo / "w.txt").write_bytes(new)
    p = c.patch("stage", "w.txt")[0].hunk_patch(0)
    if c.b.name == "git-cli":
        c.op("stage with user's apply.whitespace=fix (raw git apply)", "stage", p,
             lambda: chk_index(c, "w.txt", new), raw=True)
        c.rows[-1]["detail"] += " | expected: content altered -> must pass --whitespace=nowarn"
        c.rows[-1]["ok"] = not c.rows[-1]["ok"]  # we EXPECT the raw call to alter content
        git(repo, "reset", "-q")
    # the backend's normal path (git-cli passes --whitespace=nowarn)
    c.op("stage with --whitespace=nowarn", "stage", p, lambda: chk_index(c, "w.txt", new))


def case_turn_revert(c):
    """Revert one hunk of the *turn* diff (snapshot T1 -> T2) onto the current worktree."""
    repo = c.repo
    base = L(30)
    (repo / "t.txt").write_bytes(b"".join(base))
    commit_all(repo)
    state = STATE / f"edge-turn-{c.b.name}"
    t1 = snapshot(repo, state)
    turn = edit(base, [EDITS[0], EDITS[1]])
    (repo / "t.txt").write_bytes(turn)
    (repo / "agent-new.txt").write_bytes(b"created by agent\n")
    t2 = snapshot(repo, state)
    args = ["-R"] if c.b.reversed_diff else []
    files = hunks.parse(hunks.diff(repo, *args, t1, t2))
    c.note("turn diff includes untracked file created in the turn",
           any(b"agent-new.txt" in f.header[0] for f in files), "")
    after = edit(base, [EDITS[0], EDITS[1], ("replace", 25, b"line 25 USER-LATER\n")])
    (repo / "t.txt").write_bytes(after)
    c.op("revert turn hunk A after unrelated user edit", "revert", pick(files, b"EDIT-A"),
         lambda: chk_wt(c, "t.txt", edit(base, [EDITS[1], ("replace", 25, b"line 25 USER-LATER\n")])))
    c.op("revert turn-created untracked file", "revert", whole(files, b"agent-new.txt"),
         lambda: chk_wt(c, "agent-new.txt", None))
    (repo / "t.txt").write_bytes(edit(base, [EDITS[0], ("replace", 16, b"EDIT-B changed by user\n")]))
    c.op("revert turn hunk B after user rewrote it -> refuse", "revert", pick(files, b"EDIT-B"),
         lambda: chk_wt(c, "t.txt", edit(base, [EDITS[0], ("replace", 16, b"EDIT-B changed by user\n")])),
         expect_fail=True)


CASES = [
    ("multi_hunk", case_multi_hunk),
    ("crlf_raw", case_crlf_raw),
    ("crlf_eol_attr", case_crlf_eol_attr),
    ("crlf_autocrlf", case_crlf_autocrlf),
    ("mixed_eol", case_mixed_eol),
    ("no_eol", case_no_eol),
    ("new_file", case_new_file),
    ("empty_file", case_empty_file),
    ("deleted_file", case_deleted_file),
    ("rename", case_rename),
    ("binary", case_binary),
    ("mode_change", case_mode_change),
    ("symlink", case_symlink),
    ("special_names", case_special_names),
    ("conflict", case_conflict),
    ("whitespace_cfg", case_whitespace_cfg),
    ("turn_revert", case_turn_revert),
]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--backend", choices=["git-cli", "libgit2", "libgit2+fix"], default="git-cli",
                    help="libgit2+fix rewrites each single-hunk header to new_start=old_start first")
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    backend = CliBackend() if a.backend == "git-cli" else Libgit2Backend(fix_header=a.backend == "libgit2+fix")
    rows: List[dict] = []
    for name, fn in CASES:
        if a.only and name not in a.only.split(","):
            continue
        c = Ctx(backend, name)
        try:
            fn(c)
        except Exception as e:  # a crash in a case is a finding, keep going
            c.note("CRASH", False, f"{type(e).__name__}: {str(e)[:200]}")
        rows += c.rows
    out = {"backend": a.backend, "version": getattr(backend, "version", git_out(SCRATCH, "--version")),
           "rows": rows}
    save_result(f"edge_hunk_{a.backend}", out)
    print(md_table(["case", "op", "ok", "detail"],
                   [(r["case"], r["op"], "✅" if r["ok"] else "❌", r["detail"].replace("|", "\\|")) for r in rows]))
    bad = [r for r in rows if not r["ok"]]
    print(f"\n{len(rows) - len(bad)}/{len(rows)} ok")


if __name__ == "__main__":
    sys.exit(main())
