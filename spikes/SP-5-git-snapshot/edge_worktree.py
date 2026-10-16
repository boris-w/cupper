"""Worktree creation, safety checks before cleanup, cleanup and rebuild.

Builds  origin.git (bare)  ->  main clone  ->  linked worktrees on branches
`cupper/<name>`, then puts each worktree into a state and records:
  - what our safety check (`safety()`) reports
  - whether plain `git worktree remove` (no --force) refuses on its own

Usage: python3 edge_worktree.py
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path
from typing import List

from lib import SCRATCH, STATE, commit_all, err_text, git, git_out, md_table, new_repo, save_result
from snapshot import snapshot

ROWS: List[dict] = []
WT_ROOT = SCRATCH / "wt-root"


def row(case, check, result, detail=""):
    ROWS.append({"case": case, "check": check, "result": result, "detail": detail})


def safety(main: Path, wt: Path, branch: str) -> dict:
    """The cleanup gate proposed for GT-5. Read-only (no optional locks)."""
    r = {}
    st = git(wt, "--no-optional-locks", "status", "--porcelain=v2", "-z", "--branch",
             "--untracked-files=all", "--ignore-submodules=none").stdout.decode(errors="replace")
    ents = [e for e in st.split("\0") if e]
    r["changed"] = sum(1 for e in ents if e[:2] in ("1 ", "2 ", "u "))
    r["untracked"] = sum(1 for e in ents if e.startswith("? "))
    ign = git(wt, "--no-optional-locks", "status", "--porcelain=v2", "-z", "--ignored=matching",
              "--untracked-files=all").stdout.decode(errors="replace")
    r["ignored"] = sum(1 for e in ign.split("\0") if e.startswith("! "))
    gd = Path(git_out(wt, "rev-parse", "--path-format=absolute", "--git-dir"))
    r["in_progress"] = [n for n in ("MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge",
                                    "rebase-apply", "BISECT_LOG") if (gd / n).exists()]
    r["locked"] = (gd / "locked").exists()
    head_ref = git(wt, "symbolic-ref", "-q", "HEAD", check=False).stdout.decode().strip()
    r["head"] = head_ref.replace("refs/heads/", "") if head_ref else "DETACHED"
    r["branch_switched"] = r["head"] != branch
    # commits that exist nowhere else: reachable from the worktree HEAD but from no other ref.
    # (git keeps the branch after `worktree remove`, so this only matters for detached HEAD or
    # when we also delete the branch.)
    # NB: for --branches the --exclude pattern must NOT start with refs/heads/ (git silently
    # matches nothing otherwise - an earlier version of this check had exactly that bug).
    excl = [f"--exclude={r['head']}"] if head_ref else []
    r["unique_commits"] = int(git_out(wt, "rev-list", "--count", "HEAD", "--not", *excl, "--branches",
                                      "--remotes", "--tags"))
    has_remote = bool(git_out(main, "remote"))
    r["unpushed"] = int(git_out(wt, "rev-list", "--count", "HEAD", "--not", "--remotes")) if has_remote else None
    sub = git(wt, "submodule", "status", check=False).stdout.decode().strip()
    r["submodules_initialized"] = sum(1 for l in sub.splitlines() if l and not l.startswith("-"))
    blockers = []
    if r["changed"]:
        blockers.append("uncommitted changes")
    if r["untracked"]:
        blockers.append("untracked files")
    if r["in_progress"]:
        blockers.append("operation in progress")
    if r["locked"]:
        blockers.append("locked")
    if r["unique_commits"]:
        blockers.append("commits not on any other ref")
    if r["unpushed"]:
        blockers.append("unpushed commits")
    if r["submodules_initialized"]:
        blockers.append("initialized submodules")
    r["blockers"] = blockers
    r["safe"] = not blockers
    return r


def plain_remove(main: Path, wt: Path) -> str:
    p = git(main, "worktree", "remove", str(wt), check=False)
    if p.returncode == 0:
        return "REMOVED"
    return "refused: " + err_text(p.stderr, 110)


def setup():
    origin = new_repo("wt-origin.git", bare=True)
    seed = new_repo("wt-seed")
    (seed / "a.txt").write_text("a\n")
    (seed / ".gitignore").write_text("build/\n")
    commit_all(seed, "init")
    git(seed, "push", "-q", str(origin), "main")
    main = SCRATCH / "wt-main"
    if main.exists():
        shutil.rmtree(main)
    git(SCRATCH, "clone", "-q", str(origin), str(main))
    shutil.rmtree(WT_ROOT, ignore_errors=True)
    WT_ROOT.mkdir(parents=True)
    return origin, main


def add_wt(main: Path, name: str, base: str = "main") -> Path:
    wt = WT_ROOT / name
    git(main, "worktree", "add", "-q", "-b", f"cupper/{name}", str(wt), base)
    return wt


def state_case(main, name, prepare, expect_safe: bool):
    wt = add_wt(main, name)
    prepare(wt)
    s = safety(main, wt, f"cupper/{name}")
    row(name, "safety gate", "safe" if s["safe"] else "BLOCK: " + ", ".join(s["blockers"]),
        "ok" if s["safe"] == expect_safe else "UNEXPECTED")
    row(name, "plain `git worktree remove`", plain_remove(main, wt))
    return s


def main_() -> None:
    origin, main = setup()

    # --- states ---------------------------------------------------------------------------
    state_case(main, "clean", lambda wt: None, True)
    state_case(main, "modified", lambda wt: (wt / "a.txt").write_text("x\n"), False)

    def staged(wt):
        (wt / "s.txt").write_text("s\n")
        git(wt, "add", "s.txt")

    state_case(main, "staged", staged, False)
    state_case(main, "untracked", lambda wt: (wt / "u.txt").write_text("u\n"), False)

    def ignored(wt):
        (wt / "build").mkdir()
        (wt / "build" / "out.o").write_text("o\n")

    s = state_case(main, "ignored-only", ignored, True)
    row("ignored-only", "ignored files reported (warn, not block)", f"{s['ignored']} ignored")

    def committed(wt):
        (wt / "c.txt").write_text("c\n")
        commit_all(wt, "work")

    state_case(main, "committed-unpushed", committed, False)

    def pushed(wt):
        committed(wt)
        git(wt, "push", "-q", "origin", "HEAD:refs/heads/cupper/committed-pushed")
        git(wt, "fetch", "-q", "origin")

    state_case(main, "committed-pushed", pushed, True)

    def detached(wt):
        git(wt, "checkout", "-q", "--detach")
        (wt / "d.txt").write_text("d\n")
        commit_all(wt, "detached work")

    s = state_case(main, "detached-commits", detached, False)
    row("detached-commits", "branch switched / detached detected", s["head"])

    def merging(wt):
        (wt / "a.txt").write_text("wt side\n")
        commit_all(wt, "wt side")
        git(main, "branch", "-f", "conflict-src", "main")
        tmp = WT_ROOT / "_tmp_conflict"
        git(main, "worktree", "add", "-q", str(tmp), "conflict-src")
        (tmp / "a.txt").write_text("other side\n")
        commit_all(tmp, "other side")
        git(main, "worktree", "remove", str(tmp))
        git(wt, "merge", "conflict-src", check=False)

    state_case(main, "merge-in-progress", merging, False)

    def locked(wt):
        git(main, "worktree", "lock", "--reason", "cupper session running", str(wt))

    state_case(main, "locked", locked, False)
    git(main, "worktree", "unlock", str(WT_ROOT / "locked"))
    git(main, "worktree", "remove", "--force", str(WT_ROOT / "locked"), check=False)

    def with_submodule(wt):
        sub_src = new_repo("wt-sub-src")
        (sub_src / "lib.c").write_text("v1\n")
        commit_all(sub_src)
        git(wt, "-c", "protocol.file.allow=always", "submodule", "add", "-q", str(sub_src), "sub")
        commit_all(wt, "add sub")
        git(wt, "push", "-q", "origin", "HEAD:refs/heads/cupper/with-submodule")
        git(wt, "fetch", "-q", "origin")

    state_case(main, "with-submodule", with_submodule, False)

    # --- local-only repo: "unpushed" is meaningless, use unique commits ---------------------
    local = new_repo("wt-local")
    (local / "a.txt").write_text("a\n")
    commit_all(local)
    wt = WT_ROOT / "local-merged"
    git(local, "worktree", "add", "-q", "-b", "cupper/local-merged", str(wt))
    (wt / "x.txt").write_text("x\n")
    commit_all(wt, "x")
    s1 = safety(local, wt, "cupper/local-merged")
    git(local, "merge", "-q", "--ff-only", "cupper/local-merged")
    s2 = safety(local, wt, "cupper/local-merged")
    row("local-only repo", "unique commits before / after merging into main",
        f"{s1['unique_commits']} -> {s2['unique_commits']} (unpushed={s1['unpushed']})",
        "safe after merge" if s2["safe"] else "still blocked")
    git(local, "worktree", "remove", str(wt))

    # --- branch deletion semantics -----------------------------------------------------------
    wt = add_wt(main, "branch-del")
    committed(wt)
    git(main, "worktree", "remove", str(wt))
    p = git(main, "branch", "-d", "cupper/branch-del", check=False)
    row("branch-del", "`git branch -d` on unmerged cupper branch", "refused" if p.returncode else "DELETED",
        err_text(p.stderr, 110) if p.returncode else "")
    row("branch-del", "worktree removed but branch (and its commits) kept",
        "yes" if git(main, "rev-parse", "--verify", "-q", "cupper/branch-del", check=False).returncode == 0 else "no")

    # --- rebuild from branch -----------------------------------------------------------------
    wt = WT_ROOT / "branch-del"
    p = git(main, "worktree", "add", "-q", str(wt), "cupper/branch-del", check=False)
    row("rebuild", "`worktree add <path> <existing branch>` restores committed work",
        "ok" if p.returncode == 0 and (wt / "c.txt").exists() else "FAILED", err_text(p.stderr, 100))

    # --- manual rm -rf of a worktree dir -----------------------------------------------------
    shutil.rmtree(wt)
    lst = git_out(main, "worktree", "list", "--porcelain")
    row("rm -rf", "`worktree list` marks it prunable", "yes" if "prunable" in lst else "no")
    p = git(main, "worktree", "add", "-q", str(WT_ROOT / "elsewhere"), "cupper/branch-del", check=False)
    row("rm -rf", "re-add branch at a new path before prune", "ok" if p.returncode == 0 else "refused",
        err_text(p.stderr, 120) if p.returncode else "")
    if p.returncode == 0:
        git(main, "worktree", "remove", str(WT_ROOT / "elsewhere"))
    p = git(main, "worktree", "add", "-q", str(wt), "cupper/branch-del", check=False)
    row("rm -rf", "re-add at the SAME path before prune", "ok" if p.returncode == 0 else "refused",
        err_text(p.stderr, 120) if p.returncode else "")
    if p.returncode != 0:
        git(main, "worktree", "prune")
        p = git(main, "worktree", "add", "-q", str(wt), "cupper/branch-del", check=False)
        row("rm -rf", "after `git worktree prune`", "ok" if p.returncode == 0 else "refused")

    # --- branch already checked out elsewhere ------------------------------------------------
    p = git(main, "worktree", "add", "-q", str(WT_ROOT / "dup"), "cupper/branch-del", check=False)
    row("branch in use", "add a 2nd worktree on the same branch", "ok" if p.returncode == 0 else "refused",
        err_text(p.stderr, 120) if p.returncode else "")

    # --- branch naming -------------------------------------------------------------------------
    p = git(main, "branch", "cupper", "main", check=False)
    row("naming", "existing `cupper/*` branches block a branch named `cupper`",
        "refused" if p.returncode else "ok", err_text(p.stderr, 120) if p.returncode else "")
    naming = new_repo("wt-naming")
    (naming / "a.txt").write_text("a\n")
    commit_all(naming)
    git(naming, "branch", "cupper")
    p = git(naming, "worktree", "add", "-q", "-b", "cupper/x", str(WT_ROOT / "x"), check=False)
    row("naming", "existing branch `cupper` blocks `cupper/x` (D/F ref conflict)",
        "refused" if p.returncode else "ok", err_text(p.stderr, 120) if p.returncode else "")
    p = git(main, "worktree", "add", "-q", "-b", "cupper/branch-del", str(WT_ROOT / "y"), check=False)
    row("naming", "`-b` with an existing branch name", "refused" if p.returncode else "ok",
        err_text(p.stderr, 120) if p.returncode else "")
    for name in ["fix login", "修复-登录", "a..b", "x.lock", "ok-name_1"]:
        p = git(main, "check-ref-format", "--branch", f"cupper/{name}", check=False)
        row("naming", f"check-ref-format cupper/{name!r}", "valid" if p.returncode == 0 else "invalid")

    # --- worktree INSIDE the main repo, not ignored -------------------------------------------
    inside = main / ".cupper" / "worktrees" / "in"
    git(main, "worktree", "add", "-q", "-b", "cupper/in", str(inside))
    stt = git_out(main, "--no-optional-locks", "status", "--porcelain", "--untracked-files=all")
    row("inside repo", "main repo status shows the nested worktree", "yes" if ".cupper/" in stt else "no", stt[:80])
    t = snapshot(main, STATE / "wt-inside")
    ls = git_out(main, "ls-tree", "-r", t)
    row("inside repo", "main repo snapshot records it as gitlink (160000)",
        "yes" if "160000" in ls and ".cupper" in ls else "no")
    git(main, "worktree", "remove", "--force", str(inside))

    # --- moving the main repo breaks absolute gitdir links -----------------------------------
    wt = add_wt(main, "move-test")
    moved = SCRATCH / "wt-main-moved"
    shutil.rmtree(moved, ignore_errors=True)
    main.rename(moved)
    p = git(wt, "status", "--porcelain", check=False)
    row("move main repo", "worktree usable after the main repo moved", "ok" if p.returncode == 0 else "broken",
        err_text(p.stderr, 100) if p.returncode else "")
    p2 = git(moved, "worktree", "repair", str(wt), check=False)
    p3 = git(wt, "status", "--porcelain", check=False)
    row("move main repo", "`git worktree repair <wt>` from the moved repo fixes it",
        "ok" if p3.returncode == 0 else "still broken", err_text(p2.stderr, 100))
    moved.rename(main)
    git(main, "worktree", "repair", str(wt), check=False)
    # relative paths (git >= 2.48)
    rel_repo = new_repo("wt-relative")
    (rel_repo / "a.txt").write_text("a\n")
    commit_all(rel_repo)
    p = git(rel_repo, "worktree", "add", "-q", "--relative-paths", "-b", "cupper/rel", str(WT_ROOT / "rel"),
            check=False)
    row("relative paths", "`worktree add --relative-paths` supported", "yes" if p.returncode == 0 else "no",
        err_text(p.stderr, 100))
    ext = git(rel_repo, "config", "--get", "extensions.relativeWorktrees", check=False).stdout.decode().strip()
    fmt = git(rel_repo, "config", "--get", "core.repositoryformatversion", check=False).stdout.decode().strip()
    row("relative paths", "side effect on the user's repo config",
        f"extensions.relativeWorktrees={ext or 'unset'}, repositoryformatversion={fmt}",
        "older git / libgit2 may refuse a repo with unknown extensions")

    save_result("edge_worktree", {"rows": ROWS})
    print(md_table(["case", "check", "result", "detail"],
                   [(r["case"], r["check"], r["result"], str(r["detail"]).replace("|", "\\|")) for r in ROWS]))


if __name__ == "__main__":
    sys.exit(main_())
