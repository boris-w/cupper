"""Same operations through libgit2 (pygit2), compared with the git CLI.

  snapshot   repo.index (in memory, never written) + add_all() + write_tree()
             -> tree id compared with the CLI temp-index snapshot of the same state
  status     repo.status()
  diff       tree-to-tree diff incl. hunks (patch objects)
  apply      one hunk to the index (ApplyLocation.INDEX)
  worktree   repo.add_worktree() on a new branch (libgit2 checkout)
  filters    LFS repo: does add_all run the clean filter?

Run with uv so pygit2 (bundled libgit2) is available, e.g.
  UV_CACHE_DIR=$SP5_WORKDIR/uv-cache uv run --with pygit2 python compare_libgit2.py
"""
from __future__ import annotations

import argparse
import shutil
import sys

import pygit2

import hunks
from bench_snapshot import SCENARIOS, Scenario
from lib import REPOS, SCRATCH, STATE, file_state, git, git_out, git_path, save_result, timed
from snapshot import snapshot

EMPTY = STATE / "libgit2-empty-config"
EMPTY.mkdir(parents=True, exist_ok=True)
_levels = getattr(pygit2, "enums", None)
for lvl in ("SYSTEM", "GLOBAL", "XDG"):
    level = getattr(_levels.ConfigLevel, lvl) if _levels else getattr(pygit2, f"GIT_CONFIG_LEVEL_{lvl}")
    pygit2.settings.search_path[level] = str(EMPTY)
INDEX_LOC = _levels.ApplyLocation.INDEX if _levels else pygit2.GIT_APPLY_LOCATION_INDEX


def lg2_snapshot(path) -> str:
    r = pygit2.Repository(str(path))
    idx = r.index          # read from disk; we never call idx.write()
    idx.add_all()
    return str(idx.write_tree())


def bench_repo(name: str, runs: int) -> dict:
    repo = REPOS / name
    state = STATE / f"lg2-{name}"
    shutil.rmtree(state, ignore_errors=True)
    git(repo, "update-index", "-q", "--refresh", check=False)
    out = {"repo": name, "scenarios": {}}
    big = name in ("linux", "syn-100k")
    index_path = git_path(repo, "index")
    t0_cli = snapshot(repo, state / "t0")
    for sc_name in ("clean", "small", "medium"):
        sc = Scenario(repo, SCENARIOS[sc_name])
        res = {"changes": sc.actual}
        before = file_state(index_path)
        res["lg2_status"] = timed(lambda: pygit2.Repository(str(repo)).status(), runs=runs, setup=sc.apply)
        try:
            res["lg2_snapshot"] = timed(lambda: lg2_snapshot(repo), runs=runs, setup=sc.apply)
        except Exception as e:  # e.g. add_all() failing on the linux checkout - that is the finding
            res["lg2_snapshot_error"] = f"{type(e).__name__}: {e}"
        res["user_index_untouched"] = file_state(index_path) == before
        sc.apply(4242)
        t_cli = snapshot(repo, state / "cmp")
        if "lg2_snapshot_error" in res:
            res["same_tree_as_cli"] = None
        else:
            t_lg2 = lg2_snapshot(repo)
            res["same_tree_as_cli"] = t_lg2 == t_cli
            if t_lg2 != t_cli:
                res["tree_diff_vs_cli"] = git_out(repo, "diff", "--stat", t_cli, t_lg2)[-300:]
        r = pygit2.Repository(str(repo))

        def lg2_diff():
            d = r.diff(r[t0_cli], r[t_cli])
            d.find_similar()
            n = 0
            for p in d:
                n += len(p.hunks)
            return n

        res["lg2_diff_with_hunks"] = timed(lg2_diff, runs=runs)
        res["cli_diff_patch"] = timed(lambda: git(repo, "diff-tree", "-r", "-p", "-M", t0_cli, t_cli), runs=runs)
        if sc_name == "small":
            path = sc.modify[0]
            (repo / path).write_bytes(sc.orig[path] + b"\n/* lg2 hunk */\n")
            git(repo, "reset", "-q", "--", path)
            fp = hunks.parse(hunks.diff(repo, "--", path))[0]
            patch = fp.hunk_patch(len(fp.hunks) - 1)

            def lg2_stage():
                rr = pygit2.Repository(str(repo))
                rr.apply(pygit2.Diff.parse_diff(patch), INDEX_LOC)

            res["lg2_hunk_stage"] = timed(lg2_stage, runs=runs, setup=lambda i: git(repo, "reset", "-q", "--", path))
            res["cli_hunk_stage"] = timed(lambda: hunks.apply(repo, patch, "--cached"), runs=runs,
                                          setup=lambda i: git(repo, "reset", "-q", "--", path))
            git(repo, "reset", "-q", "--", path)
        sc.restore()
        out["scenarios"][sc_name] = res
        snap = res["lg2_snapshot"]["median_ms"] if "lg2_snapshot" in res else "ERROR"
        print(f"  {name}/{sc_name}: lg2 snap={snap}ms "
              f"status={res['lg2_status']['median_ms']}ms same_tree={res['same_tree_as_cli']}", flush=True)
    # worktree via libgit2 (checkout done by libgit2)
    wt = SCRATCH / "lg2-wt" / name
    shutil.rmtree(wt.parent, ignore_errors=True)
    wt.parent.mkdir(parents=True)
    r = pygit2.Repository(str(repo))
    n = {"i": 0}

    def lg2_add_wt():
        i = n["i"]
        n["i"] += 1
        rr = pygit2.Repository(str(repo))
        br = rr.branches.local.create(f"cupper/lg2-{i}", rr.head.peel(pygit2.Commit))
        rr.add_worktree(f"lg2-{name}-{i}", str(wt.parent / f"{name}-{i}"), br)

    def cleanup(_):
        for i in range(n["i"]):
            git(repo, "worktree", "remove", "--force", str(wt.parent / f"{name}-{i}"), check=False)
            git(repo, "branch", "-D", f"cupper/lg2-{i}", check=False)
        git(repo, "worktree", "prune")

    out["lg2_add_worktree"] = timed(lg2_add_wt, runs=3 if big else runs, warmup=1, setup=cleanup)
    last = wt.parent / f"{name}-{n['i'] - 1}"
    st = git_out(last, "--no-optional-locks", "status", "--porcelain")
    out["lg2_worktree_clean_per_cli"] = not st or name == "linux"
    out["lg2_worktree_status_head"] = st[:200]
    cleanup(None)
    return out


def lfs_check() -> dict:
    """libgit2 does not run external filter drivers -> raw content ends up in the snapshot."""
    repo = SCRATCH / "snap-lfs"
    if not repo.exists():
        return {"skipped": "run edge_snapshot.py --only lfs first"}
    (repo / "big.bin").write_bytes(b"lg2 " * 1000)
    t_lg2 = lg2_snapshot(repo)
    blob = pygit2.Repository(str(repo))[t_lg2] / "big.bin"
    data = blob.data if hasattr(blob, "data") else pygit2.Repository(str(repo))[blob.id].data
    t_cli = snapshot(repo, STATE / "lg2-lfs-cli")
    cli_blob = git(repo, "cat-file", "-p", f"{t_cli}:big.bin").stdout
    git(repo, "checkout", "--", "big.bin", check=False)
    return {"lg2_blob_is_pointer": data.startswith(b"version https://git-lfs"),
            "lg2_blob_size": len(data), "cli_blob_is_pointer": cli_blob.startswith(b"version https://git-lfs"),
            "cli_blob_size": len(cli_blob)}


def compat_check() -> list:
    """Open the special repos created by edge_snapshot.py / edge_worktree.py with libgit2."""
    out = []
    probes = [
        ("split index (core.splitIndex)", SCRATCH / "snap-split", "snapshot"),
        ("sparse checkout (skip-worktree)", SCRATCH / "snap-sparse", "snapshot"),
        ("relative worktrees extension", SCRATCH / "wt-relative", "open"),
        ("linked worktree created by git", SCRATCH / "snap-wt-linked", "snapshot"),
    ]
    for label, path, what in probes:
        if not path.exists():
            out.append({"case": label, "result": "skipped (run edge scripts first)"})
            continue
        try:
            r = pygit2.Repository(str(path))
            if what == "open":
                r.status()
                out.append({"case": label, "result": "ok"})
                continue
            t_lg2 = lg2_snapshot(path)
            t_cli = snapshot(path, STATE / f"lg2-compat-{path.name}")
            same = t_lg2 == t_cli
            detail = "" if same else git_out(path, "diff", "--stat", t_cli, t_lg2)[-200:]
            out.append({"case": label, "result": "same tree as CLI" if same else "DIFFERENT tree", "detail": detail})
        except Exception as e:  # libgit2 refusing the repo is the finding
            out.append({"case": label, "result": "ERROR", "detail": f"{type(e).__name__}: {str(e)[:160]}"})
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repos", default="syn-1k,syn-10k,syn-100k,linux")
    ap.add_argument("--runs", type=int, default=5)
    a = ap.parse_args()
    res = {"version": f"pygit2 {pygit2.__version__} / libgit2 {pygit2.LIBGIT2_VERSION}",
           "features": {k: getattr(pygit2, f"GIT_FEATURE_{k}", None) is not None for k in ("THREADS", "HTTPS", "SSH")},
           "repos": []}
    print(res["version"])
    for name in a.repos.split(","):
        res["repos"].append(bench_repo(name, a.runs))
        save_result("compare_libgit2", res)
    res["lfs"] = lfs_check()
    print("lfs:", res["lfs"])
    res["compat"] = compat_check()
    for c in res["compat"]:
        print("compat:", c)
    save_result("compare_libgit2", res)


if __name__ == "__main__":
    sys.exit(main())
