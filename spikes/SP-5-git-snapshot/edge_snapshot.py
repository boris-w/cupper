"""Boundary cases for turn snapshots (temp index + add -A + write-tree).

Each case builds a small repo under $SP5_WORKDIR/scratch and records what the
snapshot does, whether the user's index stayed untouched, and how
`git stash create` behaves in the same situation.

Usage: python3 edge_snapshot.py [--only case1,case2] [--big-mb 1024]
"""
from __future__ import annotations

import argparse
import os
import shutil
import socket
import sys
import time
from pathlib import Path
from typing import List

from lib import (GitError, STATE, TOOLS_BIN, commit_all, err_text, file_state, git, git_out, git_path, md_table,
                 new_repo, save_result)
from snapshot import anchor, odb_env, snapshot, stash_create

ROWS: List[dict] = []


def row(case: str, check: str, ok, detail: str = "") -> None:
    ROWS.append({"case": case, "check": check, "ok": ok, "detail": detail})


def tree_paths(repo, tree: str, env=None) -> dict:
    out = git(repo, "ls-tree", "-r", "-z", tree, env=env).stdout.decode(errors="replace")
    res = {}
    for ent in out.split("\0"):
        if ent:
            meta, path = ent.split("\t", 1)
            mode, typ, oid = meta.split()
            res[path] = (mode, typ, oid)
    return res


def try_snapshot(repo, state, **kw):
    try:
        return snapshot(repo, state, **kw), ""
    except GitError as e:
        return None, e.stderr.strip().splitlines()[-1] if e.stderr.strip() else str(e)


def try_stash(repo):
    try:
        return stash_create(repo), ""
    except GitError as e:
        return None, e.stderr.strip().splitlines()[-1] if e.stderr.strip() else str(e)


def st(name):
    p = STATE / f"edge-snap-{name}"
    shutil.rmtree(p, ignore_errors=True)
    return p


# --------------------------------------------------------------------------- cases


def case_index_lock():
    """User has a git command in flight (.git/index.lock exists)."""
    c = "index.lock held by user"
    repo = new_repo("snap-lock")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    (repo / "a.txt").write_text("a2\n")
    (repo / "u.txt").write_text("u\n")
    lock = git_path(repo, "index.lock")
    lock.write_bytes(b"")
    before = file_state(git_path(repo, "index"))
    t, err = try_snapshot(repo, st("lock"))
    row(c, "temp-index snapshot succeeds", t is not None, err)
    row(c, "user index + lock untouched", file_state(git_path(repo, "index")) == before and lock.exists())
    s, err = try_stash(repo)
    row(c, "git stash create", s is not None, err or "succeeded")
    lock.unlink()


def case_unmerged():
    """Index has conflict stages (merge in progress)."""
    c = "merge conflict in index"
    repo = new_repo("snap-unmerged")
    (repo / "f.txt").write_text("base\n")
    commit_all(repo)
    git(repo, "checkout", "-q", "-b", "other")
    (repo / "f.txt").write_text("other\n")
    commit_all(repo)
    git(repo, "checkout", "-q", "main")
    (repo / "f.txt").write_text("main\n")
    commit_all(repo)
    git(repo, "merge", "other", check=False)
    stages = git_out(repo, "ls-files", "-u")
    row(c, "index has stage 1/2/3 entries", bool(stages), f"{len(stages.splitlines())} unmerged entries")
    before = file_state(git_path(repo, "index"))
    t, err = try_snapshot(repo, st("unmerged"))
    content = git_out(repo, "cat-file", "-p", f"{t}:f.txt") if t else ""
    row(c, "snapshot succeeds (conflict markers captured as content)", t is not None and "<<<<<<<" in content, err)
    row(c, "user index untouched (still unmerged)", file_state(git_path(repo, "index")) == before)
    s, err = try_stash(repo)
    row(c, "git stash create", s is not None, err or "succeeded")


def case_ignore_rules():
    c = "untracked + ignore rules"
    repo = new_repo("snap-ignore")
    (repo / ".gitignore").write_text("*.log\nbuild/\n!keep.log\n")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    (git_path(repo, "info") / "exclude").write_text("secret.env\n")
    excl = STATE / "edge-excludes"
    excl.write_text("*.tmp\n")
    git(repo, "config", "core.excludesFile", str(excl))
    for p, d in {"new.txt": "n", "x.log": "l", "keep.log": "k", "build/out.o": "o", "secret.env": "s",
                 "scratch.tmp": "t", "sub/deep/new2.txt": "n2"}.items():
        (repo / p).parent.mkdir(parents=True, exist_ok=True)
        (repo / p).write_text(d + "\n")
    os.chmod(repo / "new.txt", 0o755)
    t, err = try_snapshot(repo, st("ignore"))
    paths = tree_paths(repo, t) if t else {}
    want_in = {"new.txt", "keep.log", "sub/deep/new2.txt"}
    want_out = {"x.log", "build/out.o", "secret.env", "scratch.tmp"}
    row(c, "untracked files included", want_in <= set(paths), ", ".join(sorted(want_in & set(paths))))
    row(c, ".gitignore / info/exclude / core.excludesFile / negation honored", not (want_out & set(paths)),
        "leaked: " + ", ".join(sorted(want_out & set(paths))) if want_out & set(paths) else "")
    row(c, "exec bit recorded", paths.get("new.txt", ("",))[0] == "100755", paths.get("new.txt", ("?",))[0])
    s, _ = try_stash(repo)
    has_untracked = bool(s) and "new.txt" in git_out(repo, "ls-tree", "-r", "--name-only", s)
    row(c, "git stash create includes untracked files", has_untracked, f"stash={s[:10] if s else s!r}")


def case_nested_repo():
    c = "nested repo (untracked dir with its own .git)"
    repo = new_repo("snap-nested")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    inner = repo / "vendor" / "lib"
    inner.mkdir(parents=True)
    git(inner, "init", "-q")
    (inner / "x.c").write_text("x\n")
    t, err = try_snapshot(repo, st("nested0"))
    row(c, "inner repo WITHOUT commits: plain add -A", t is not None, err)
    t, err = try_snapshot(repo, st("nested0b"), ignore_errors=True)
    row(c, "... with --ignore-errors", t is not None, err)
    # mitigation: `ls-files -o` (without --directory) does not descend into nested repos and
    # lists each of them as 'path/' -> exclude those paths from the snapshot.
    ents = git(repo, "ls-files", "-o", "--exclude-standard", "-z").stdout.decode().split("\0")
    nested = [e.rstrip("/") for e in ents if e.endswith("/") and (repo / e / ".git").exists()]
    t, err = try_snapshot(repo, st("nested0c"), exclude=nested)
    row(c, "... with nested repo dirs excluded by pathspec", t is not None, f"excluded {nested}" + (err and f" ({err})"))
    commit_all(inner)
    t1, err = try_snapshot(repo, st("nested1"))
    paths = tree_paths(repo, t1) if t1 else {}
    ent = paths.get("vendor/lib")
    row(c, "inner repo WITH a commit -> recorded as gitlink (160000)", bool(ent) and ent[0] == "160000",
        f"{ent}" if ent else err)
    (inner / "x.c").write_text("changed inside\n")
    t2, _ = try_snapshot(repo, st("nested1"))
    row(c, "uncommitted change inside nested repo visible in snapshot diff", t1 != t2,
        "no: the gitlink only records the inner HEAD" if t1 == t2 else "")


def case_submodule(src_repo_dir: Path):
    c = "submodule"
    sub_src = new_repo("snap-sub-src")
    (sub_src / "lib.c").write_text("v1\n")
    commit_all(sub_src)
    repo = new_repo("snap-sub")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    git(repo, "-c", "protocol.file.allow=always", "submodule", "add", "-q", str(sub_src), "sub")
    git(repo, "commit", "-q", "-m", "add sub")
    t0, _ = try_snapshot(repo, st("sub"))
    (repo / "sub" / "lib.c").write_text("dirty\n")
    t1, err = try_snapshot(repo, st("sub"))
    row(c, "dirty file inside submodule changes the snapshot", t0 != t1,
        "no: only the gitlink (submodule HEAD) is recorded" if t0 == t1 else err)
    stat = git_out(repo, "--no-optional-locks", "status", "--porcelain=v2")
    row(c, "status --porcelain=v2 reports the submodule as modified", " S.M" in stat or "S..M" in stat or "S.M." in stat,
        stat.splitlines()[0][:60] if stat else "")
    commit_all(repo / "sub", "inner commit")
    t2, _ = try_snapshot(repo, st("sub"))
    d = git_out(repo, "diff", "--no-color", t0, t2)
    row(c, "new commit inside submodule shows as gitlink change", "Subproject commit" in d,
        d.splitlines()[-1] if d else "")
    d2 = git_out(repo, "diff", "--no-color", "--submodule=log", t0, t2)
    row(c, "--submodule=log summarises inner commits", "inner commit" in d2 or ">" in d2, d2.splitlines()[0][:70] if d2 else "")
    # stage / unstage the gitlink with a patch
    p = git(repo, "diff", "--no-color", "--full-index", "--", "sub").stdout
    try:
        git(repo, "apply", "--cached", "-", input=p)
        ok = git_out(repo, "ls-files", "-s", "sub").split()[1] == git_out(repo / "sub", "rev-parse", "HEAD")
        row(c, "stage gitlink change via git apply --cached", ok)
    except GitError as e:
        row(c, "stage gitlink change via git apply --cached", False, e.stderr.strip()[:120])


def case_lfs():
    c = "Git LFS"
    if not (TOOLS_BIN / "git-lfs").exists() and not shutil.which("git-lfs"):
        row(c, "git-lfs available", False, "skipped: git-lfs not found")
        return
    repo = new_repo("snap-lfs")
    git(repo, "lfs", "install", "--local")
    git(repo, "lfs", "track", "*.bin")
    data = os.urandom(5 << 20)
    (repo / "big.bin").write_bytes(data)
    commit_all(repo)
    (repo / "big.bin").write_bytes(os.urandom(5 << 20))
    lfs_before = sum(1 for _ in (git_path(repo, "lfs") / "objects").rglob("*") if _.is_file())
    t, err = try_snapshot(repo, st("lfs"))
    blob = git_out(repo, "cat-file", "-p", f"{t}:big.bin") if t else ""
    lfs_after = sum(1 for _ in (git_path(repo, "lfs") / "objects").rglob("*") if _.is_file())
    row(c, "snapshot stores the LFS pointer, not the content", blob.startswith("version https://git-lfs"),
        blob.splitlines()[1] if blob else err)
    row(c, "side effect: clean filter copies content into .git/lfs/objects", lfs_after > lfs_before,
        f"{lfs_before} -> {lfs_after} files")
    d = git_out(repo, "diff", "--no-color", "HEAD", t)
    row(c, "turn diff shows pointer change only", "oid sha256:" in d, "")
    # GUI apps on macOS start with PATH=/usr/bin:/bin:/usr/sbin:/sbin (no Homebrew) -> no git-lfs
    gui_env = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin"}
    t2, err = try_snapshot(repo, st("lfs-nopath"), env=gui_env)
    row(c, "snapshot when git-lfs is NOT on PATH (GUI app PATH)", t2 is not None, err[:150])
    s2 = git(repo, "--no-optional-locks", "status", "--porcelain", env=gui_env, check=False)
    row(c, "git status when git-lfs is NOT on PATH", s2.returncode == 0,
        err_text(s2.stderr, 150, last=True) if s2.stderr.strip() else "ok")


def case_big_files(big_mb: int):
    c = "large untracked files"
    repo = new_repo("snap-big")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    res = {}
    for label, mb, gen in [("100MB random", 100, "rand"), (f"{big_mb}MB random", big_mb, "rand"),
                           (f"{big_mb}MB zeros", big_mb, "zero")]:
        path = repo / "big.dat"
        with open(path, "wb") as f:
            chunk = os.urandom(1 << 20) if gen == "rand" else b"\0" * (1 << 20)
            for i in range(mb):
                f.write(chunk if gen == "zero" else os.urandom(1 << 20))
        before = git_out(repo, "count-objects", "-v")
        t = time.perf_counter()
        tree, err = try_snapshot(repo, st("big"))
        ms = (time.perf_counter() - t) * 1000
        after = git_out(repo, "count-objects", "-v")

        def size(s, key):
            return int([l for l in s.splitlines() if l.startswith(key + ":")][0].split()[1])

        loose = size(after, "size") - size(before, "size")
        pack = size(after, "size-pack") - size(before, "size-pack")
        row(c, f"snapshot with one {label} untracked file", tree is not None,
            f"{ms:.0f} ms, object store +{(loose + pack) / 1024:.0f} MB "
            f"({'pack: above core.bigFileThreshold' if pack > loose else 'loose'})" + (f" ({err})" if err else ""))
        res[label] = ms
        path.unlink()
        git(repo, "prune", "--expire=now")
    # pre-filter: list untracked files with sizes, exclude anything above a threshold
    path = repo / "big.dat"
    with open(path, "wb") as f:
        for i in range(big_mb):
            f.write(os.urandom(1 << 20))
    t = time.perf_counter()
    untracked = git(repo, "ls-files", "-o", "--exclude-standard", "-z").stdout.decode().split("\0")
    too_big = [u for u in untracked if u and (repo / u).stat().st_size > (50 << 20)]
    tree, err = try_snapshot(repo, st("big-ex"), exclude=too_big)
    ms = (time.perf_counter() - t) * 1000
    paths = tree_paths(repo, tree) if tree else {}
    row(c, f"same {big_mb}MB file with size pre-filter (>50MB excluded)", tree is not None and "big.dat" not in paths,
        f"{ms:.0f} ms incl. ls-files -o + stat" + (f" ({err})" if err else ""))
    path.unlink()


def case_many_untracked():
    c = "many untracked files (e.g. a non-ignored node_modules)"
    repo = new_repo("snap-many")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    n = 20000
    for i in range(n):
        d = repo / "node_modules" / f"p{i // 100:03d}"
        if i % 100 == 0:
            d.mkdir(parents=True, exist_ok=True)
        (d / f"f{i}.js").write_text(f"module.exports = {i};\n")
    t = time.perf_counter()
    tree, err = try_snapshot(repo, st("many"))
    ms = (time.perf_counter() - t) * 1000
    t = time.perf_counter()
    cnt = git(repo, "ls-files", "-o", "--exclude-standard", "--directory", "-z").stdout.count(b"\0")
    ms_ls = (time.perf_counter() - t) * 1000
    row(c, f"first snapshot with {n} untracked files", tree is not None, f"{ms:.0f} ms" + (f" ({err})" if err else ""))
    row(c, "cheap pre-check: ls-files -o --directory", True, f"{ms_ls:.0f} ms, {cnt} top-level untracked entries")
    t = time.perf_counter()
    cnt = git(repo, "ls-files", "-o", "--exclude-standard", "-z").stdout.count(b"\0")
    row(c, "full pre-check: ls-files -o (every untracked file)", True,
        f"{(time.perf_counter() - t) * 1000:.0f} ms, {cnt} files")
    t = time.perf_counter()
    tree2, _ = try_snapshot(repo, st("many"), seed="keep")
    row(c, "next snapshot with kept temp index (no new content)", tree2 == tree,
        f"{(time.perf_counter() - t) * 1000:.0f} ms")


def case_special_files():
    c = "FIFO / socket / unreadable file"
    repo = new_repo("snap-special")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    os.mkfifo(repo / "pipe")
    s = socket.socket(socket.AF_UNIX)
    sock_path = repo / "dev.sock"
    try:
        s.bind(str(sock_path))
    except OSError:
        sock_path = None
    t, err = try_snapshot(repo, st("special"))
    paths = tree_paths(repo, t) if t else {}
    row(c, "FIFO + unix socket in worktree: snapshot succeeds", t is not None,
        err or f"recorded: {sorted(p for p in paths if p != 'a.txt') or 'nothing'}")
    s.close()
    os.unlink(repo / "pipe")
    if sock_path and sock_path.exists():
        sock_path.unlink()
    (repo / "secret.txt").write_text("nope\n")
    os.chmod(repo / "secret.txt", 0)
    t, err = try_snapshot(repo, st("special2"))
    row(c, "unreadable untracked file: plain add -A", t is not None, err)
    e = {"GIT_INDEX_FILE": str(st("special3") / "i")}
    (STATE / "edge-snap-special3").mkdir(parents=True, exist_ok=True)
    p = git(repo, "add", "-A", "--ignore-errors", env=e, check=False)
    tree = git(repo, "write-tree", env=e, check=False).stdout.decode().strip()
    row(c, "... with --ignore-errors: exit code / tree written", bool(tree),
        f"add exit={p.returncode}; tree={'yes' if tree else 'no'}; stderr: "
        f"{err_text(p.stderr, 80) if p.stderr.strip() else ''}")
    os.chmod(repo / "secret.txt", 0o644)


def case_linked_worktree():
    c = "snapshot inside a linked worktree"
    repo = new_repo("snap-wt")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    wt = repo.parent / "snap-wt-linked"
    shutil.rmtree(wt, ignore_errors=True)
    git(repo, "worktree", "prune")
    git(repo, "worktree", "add", "-q", "-b", "cupper/x", str(wt))
    (wt / "a.txt").write_text("changed in wt\n")
    (wt / "n.txt").write_text("new\n")
    idx = git_path(wt, "index")
    before, before_main = file_state(idx), file_state(git_path(repo, "index"))
    t, err = try_snapshot(wt, st("linked"))
    paths = tree_paths(wt, t) if t else {}
    row(c, "snapshot works, uses <common-dir>/worktrees/<name>/index as seed", t is not None and "n.txt" in paths,
        str(idx).split("/.git/")[-1] if t else err)
    row(c, "neither the worktree's nor the main index is touched",
        file_state(idx) == before and file_state(git_path(repo, "index")) == before_main)


def case_split_index():
    c = "core.splitIndex=true"
    repo = new_repo("snap-split")
    for i in range(50):
        (repo / f"f{i}.txt").write_text(f"{i}\n")
    commit_all(repo)
    git(repo, "config", "core.splitIndex", "true")
    git(repo, "update-index", "--split-index")
    (repo / "f1.txt").write_text("x\n")
    git(repo, "add", "f1.txt")
    shared = list(git_path(repo, "index").parent.glob("sharedindex.*"))
    (repo / "f2.txt").write_text("y\n")
    (repo / "new.txt").write_text("n\n")
    t, err = try_snapshot(repo, st("split"))
    paths = tree_paths(repo, t) if t else {}
    row(c, "seeded snapshot works with a split index", t is not None and "new.txt" in paths and len(paths) == 51,
        f"sharedindex files={len(shared)}; " + (f"{len(paths)} paths" if t else err))


def case_sparse():
    c = "sparse-checkout (skip-worktree entries)"
    repo = new_repo("snap-sparse")
    for d in ("keep", "skip"):
        for i in range(3):
            (repo / d / f"{i}.txt").parent.mkdir(parents=True, exist_ok=True)
            (repo / d / f"{i}.txt").write_text(f"{d}{i}\n")
    commit_all(repo)
    git(repo, "sparse-checkout", "set", "keep")
    row(c, "skip/ removed from worktree", not (repo / "skip").exists())
    (repo / "keep" / "0.txt").write_text("changed\n")
    t, _ = try_snapshot(repo, st("sparse"))
    paths = tree_paths(repo, t)
    row(c, "seeded snapshot keeps skip-worktree files (not shown as deleted)", "skip/0.txt" in paths)
    te, _ = try_snapshot(repo, st("sparse-empty"), seed="empty")
    pe = tree_paths(repo, te)
    row(c, "EMPTY-seeded snapshot treats them as deleted", "skip/0.txt" not in pe,
        "=> always seed from the user's index (or a kept temp index)")


def case_assume_unchanged():
    c = "assume-unchanged / skip-worktree set by the user"
    repo = new_repo("snap-assume")
    (repo / "cfg.txt").write_text("v1\n")
    commit_all(repo)
    git(repo, "update-index", "--assume-unchanged", "cfg.txt")
    t0, _ = try_snapshot(repo, st("assume"))
    (repo / "cfg.txt").write_text("v2 edited by agent\n")
    t1, _ = try_snapshot(repo, st("assume"))
    row(c, "seeded snapshot sees the edit", t0 != t1,
        "no: flag is copied with the index; add -A skips the file" if t0 == t1 else "")
    git(repo, "update-index", "--no-assume-unchanged", "cfg.txt")


def case_safecrlf():
    c = "core.safecrlf=true + mixed EOL file"
    repo = new_repo("snap-safecrlf")
    (repo / ".gitattributes").write_text("*.txt text eol=lf\n")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    git(repo, "config", "core.safecrlf", "true")
    (repo / "m.txt").write_bytes(b"one\r\ntwo\n")
    e = {"GIT_INDEX_FILE": str(st("safecrlf-raw") / "i")}
    (STATE / "edge-snap-safecrlf-raw").mkdir(parents=True, exist_ok=True)
    p = git(repo, "add", "-A", env=e, check=False)
    row(c, "plain `git add -A` with user's safecrlf=true", p.returncode == 0,
        err_text(p.stderr, 100, last=True) if p.returncode else "ok")
    t, err = try_snapshot(repo, st("safecrlf"))
    row(c, "snapshot with -c core.safecrlf=false", t is not None, err)


def case_unborn():
    c = "empty repo (no commits yet)"
    repo = new_repo("snap-unborn")
    (repo / "a.txt").write_text("a\n")
    t, err = try_snapshot(repo, st("unborn"))
    row(c, "snapshot works without HEAD / index", t is not None, err)
    s, err = try_stash(repo)
    row(c, "git stash create", s is not None, err or "succeeded")


def case_gc_and_private_odb():
    c = "object lifetime: gc, anchor ref, private object dir"
    repo = new_repo("snap-gc")
    (repo / "a.txt").write_text("a\n")
    commit_all(repo)
    (repo / "u.txt").write_text("only in snapshot " + str(time.time_ns()) + "\n")
    t, _ = try_snapshot(repo, st("gc"))
    git(repo, "gc", "-q", "--prune=now")
    alive = git(repo, "cat-file", "-e", t, check=False).returncode == 0
    row(c, "unanchored snapshot survives `git gc --prune=now`", alive,
        "no: unreachable objects are pruned (default grace period 2 weeks)" if not alive else "")
    (repo / "u.txt").write_text("anchored " + str(time.time_ns()) + "\n")
    t, _ = try_snapshot(repo, st("gc"))
    anchor(repo, t, "refs/cupper/snapshots/s1/t1-end")
    git(repo, "gc", "-q", "--prune=now")
    alive = git(repo, "cat-file", "-e", t, check=False).returncode == 0
    row(c, "snapshot anchored by refs/cupper/... survives gc", alive)
    log_all = git_out(repo, "log", "--all", "--oneline")
    row(c, "... but `git log --all` now shows cupper snapshot commits", "cupper snapshot" not in log_all,
        f"{log_all.count('cupper snapshot')} snapshot commit(s) visible in log --all")
    git(repo, "update-ref", "-d", "refs/cupper/snapshots/s1/t1-end")
    # private object dir
    odb = STATE / "edge-snap-gc-odb"
    shutil.rmtree(odb, ignore_errors=True)
    before = git_out(repo, "count-objects", "-v")
    (repo / "u.txt").write_text("private " + str(time.time_ns()) + "\n")
    t, err = try_snapshot(repo, st("gc"), private_odb=odb)
    after = git_out(repo, "count-objects", "-v")
    row(c, "private ODB: nothing written to the repo's .git/objects", before == after and t is not None, err)
    e = odb_env(repo, odb)
    ok_with = git(repo, "diff", "--stat", "HEAD", t, env=e, check=False).returncode == 0
    ok_without = git(repo, "diff", "--stat", "HEAD", t, check=False).returncode == 0
    row(c, "private ODB: diff works with env, fails without", ok_with and not ok_without,
        f"with env={ok_with}, without={ok_without}")
    git(repo, "gc", "-q", "--prune=now")
    row(c, "private ODB: user's gc cannot prune snapshot objects",
        git(repo, "cat-file", "-e", t, env=e, check=False).returncode == 0)
    # user rewrites history + aggressive gc -> a blob only referenced by HEAD~ disappears
    (repo / "gone.txt").write_text("will be rewritten away " + str(time.time_ns()) + "\n")
    commit_all(repo, "to be dropped")
    t_old, _ = try_snapshot(repo, st("gc"), private_odb=odb)
    git(repo, "reset", "-q", "--hard", "HEAD~")
    git(repo, "reflog", "expire", "--expire=now", "--all")
    git(repo, "gc", "-q", "--prune=now")
    ok = git(repo, "diff", "--stat", "HEAD", t_old, env=e, check=False).returncode == 0
    row(c, "private ODB: snapshot still diffable after user drops a commit + gc --prune=now", ok,
        "no: blobs that only lived in the dropped commit are gone -> show 'snapshot unavailable'" if not ok else "")


def case_case_collision():
    c = "case-only path collision on case-insensitive APFS"
    repo = new_repo("snap-case")
    a = git_out(repo, "hash-object", "-w", "--stdin", input=b"upper\n")
    b = git_out(repo, "hash-object", "-w", "--stdin", input=b"lower\n")
    git(repo, "update-index", "--add", "--cacheinfo", f"100644,{a},README.md")
    git(repo, "update-index", "--add", "--cacheinfo", f"100644,{b},readme.md")
    git(repo, "commit", "-q", "-m", "both")
    git(repo, "checkout", "-q", "-f", "HEAD")
    dirty = git_out(repo, "--no-optional-locks", "status", "--porcelain")
    row(c, "fresh checkout is clean", not dirty, f"status: {dirty!r}" if dirty else "")
    t0, _ = try_snapshot(repo, st("case"))
    t1, _ = try_snapshot(repo, st("case"))
    row(c, "two consecutive snapshots are identical (turn diff not polluted)", t0 == t1)


CASES = {
    "index_lock": case_index_lock, "unmerged": case_unmerged, "ignore_rules": case_ignore_rules,
    "nested_repo": case_nested_repo, "submodule": lambda: case_submodule(None), "lfs": case_lfs,
    "big_files": None, "many_untracked": case_many_untracked, "special_files": case_special_files,
    "linked_worktree": case_linked_worktree, "split_index": case_split_index, "sparse": case_sparse,
    "assume_unchanged": case_assume_unchanged, "safecrlf": case_safecrlf, "unborn": case_unborn,
    "gc_private_odb": case_gc_and_private_odb, "case_collision": case_case_collision,
}


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--only", default="")
    ap.add_argument("--big-mb", type=int, default=1024)
    a = ap.parse_args()
    for name, fn in CASES.items():
        if a.only and name not in a.only.split(","):
            continue
        try:
            if name == "big_files":
                case_big_files(a.big_mb)
            else:
                fn()
        except Exception as e:
            row(name, "CRASH", False, f"{type(e).__name__}: {str(e)[:300]}")
    save_result("edge_snapshot" + ("_" + a.only.replace(",", "_") if a.only else ""), {"rows": ROWS})
    print(md_table(["case", "check", "result", "detail"],
                   [(r["case"], r["check"], "yes" if r["ok"] else "NO", str(r["detail"]).replace("|", "\\|"))
                    for r in ROWS]))


if __name__ == "__main__":
    sys.exit(main())
