"""Benchmark turn snapshots, snapshot diffs and single-hunk index/worktree ops.

For each repo and change scenario (clean / small / medium / large) we measure,
median of N runs (each run writes *new* content, so blobs are new every time):

  status        git --no-optional-locks status --porcelain=v2 -z --untracked-files=all
  snap_seed     temp index copied from the user's index + add -A + write-tree
  snap_keep     temp index kept from the previous snapshot of the session
  snap_private  snap_seed, but new objects go to a private object dir (alternates)
  snap_empty    empty temp index (hashes every file)          [clean scenario only]
  stash_create  git stash create (no untracked files!)
  diff_*        tree-to-tree diffs between the clean snapshot and the scenario snapshot
  hunk_*        stage / unstage / revert a single hunk with git apply   [small scenario]

The user's index (<git-dir>/index) is hashed before and after every snapshot run
to prove the snapshot never touches it.

Usage: python3 bench_snapshot.py [--repos syn-1k,syn-10k,syn-100k,linux] [--runs 5] [--no-fsmonitor]
"""
from __future__ import annotations

import argparse
import random
import shutil
import sys
import time
from pathlib import Path

import hunks
from lib import REPOS, STATE, file_state, git, git_out, git_path, save_result, timed
from snapshot import snapshot, snapshot_guided, stash_create

SCENARIOS = {
    "clean": dict(modify=0, add=0, delete=0),
    "small": dict(modify=10, add=5, delete=1),
    "medium": dict(modify=1000, add=100, delete=10),
    "large": dict(modify=10000, add=1000, delete=100),
}
UNTRACKED_DIR = "sp5-untracked"


class Scenario:
    """Applies a change set with a per-run salt so every run produces new blobs."""

    def __init__(self, repo: Path, spec: dict, seed: int = 1):
        self.repo = repo
        files = [f for f in git(repo, "ls-files", "-z").stdout.decode(errors="surrogateescape").split("\0") if f]
        rng = random.Random(seed)
        regular = [f for f in files if (repo / f).is_file() and not (repo / f).is_symlink()]
        cap = len(regular) // 2
        n_mod = min(spec["modify"], cap)
        n_del = min(spec["delete"], cap // 10)
        picked = rng.sample(regular, n_mod + n_del)
        self.modify = picked[:n_mod]
        self.delete = picked[n_mod:]
        self.n_add = spec["add"]
        self.orig = {f: (repo / f).read_bytes() for f in picked}
        self.actual = dict(modify=n_mod, add=self.n_add, delete=n_del)

    def apply(self, salt: int) -> None:
        tag = f"\n/* sp5 salt {salt} {time.time_ns()} */\n".encode()
        for f in self.modify:
            (self.repo / f).write_bytes(self.orig[f] + tag)
        for f in self.delete:
            p = self.repo / f
            if p.exists():
                p.unlink()
        if self.n_add:
            d = self.repo / UNTRACKED_DIR
            d.mkdir(exist_ok=True)
            for k in range(self.n_add):
                (d / f"u{k:05d}.txt").write_bytes(b"untracked %d\n" % k + tag * 20)

    def restore(self) -> None:
        for f, data in self.orig.items():
            p = self.repo / f
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
        shutil.rmtree(self.repo / UNTRACKED_DIR, ignore_errors=True)
        git(self.repo, "update-index", "-q", "--refresh", check=False)


def status(repo: Path, untracked: str = "all") -> bytes:
    return git(repo, "--no-optional-locks", "status", "--porcelain=v2", "-z", f"--untracked-files={untracked}").stdout


def count_objects(repo: Path, env=None) -> dict:
    out = git_out(repo, "count-objects", "-v", env=env)
    return {k: int(v) for k, v in (line.split(": ") for line in out.splitlines()) if v.isdigit()}


def bench_repo(name: str, runs: int, scenarios) -> dict:
    repo = REPOS / name
    state = STATE / name
    shutil.rmtree(state, ignore_errors=True)
    state.mkdir(parents=True)
    index_path = git_path(repo, "index")
    git(repo, "update-index", "-q", "--refresh", check=False)
    out = {"repo": name, "scenarios": {}}

    # clean baseline snapshot used as T0 for every diff
    t0 = snapshot(repo, state / "t0")
    small_runs = runs
    big = name in ("linux", "syn-100k")

    for sc_name in scenarios:
        spec = SCENARIOS[sc_name]
        sc = Scenario(repo, spec)
        res = {"changes": sc.actual}
        before = file_state(index_path)

        res["status"] = timed(lambda: status(repo), runs=small_runs, setup=sc.apply)
        res["snap_seed"] = timed(lambda: snapshot(repo, state / "seed"), runs=small_runs, setup=sc.apply)
        snapshot(repo, state / "keep")  # first snapshot of the "session"
        res["snap_keep"] = timed(lambda: snapshot(repo, state / "keep", seed="keep"), runs=small_runs,
                                 setup=sc.apply)
        res["snap_private"] = timed(lambda: snapshot(repo, state / "priv", private_odb=state / "odb"),
                                    runs=small_runs, setup=sc.apply)
        res["snap_guided"] = timed(lambda: snapshot_guided(repo, state / "guided"), runs=small_runs,
                                   setup=sc.apply)
        if sc_name == "clean":
            res["snap_empty"] = timed(lambda: snapshot(repo, state / "empty", seed="empty"),
                                      runs=3 if big else small_runs, setup=sc.apply)
        after_snap = file_state(index_path)
        res["user_index_untouched"] = after_snap == before
        res["stash_create"] = timed(lambda: stash_create(repo), runs=small_runs, setup=sc.apply)
        res["stash_create_touched_index"] = file_state(index_path) != after_snap

        # object database growth for one snapshot with fresh content
        sc.apply(10_000)
        c0 = count_objects(repo)
        t1 = snapshot(repo, state / "seed")
        c1 = count_objects(repo)
        res["odb_growth"] = {"loose_objects": c1["count"] - c0["count"], "loose_kb": c1["size"] - c0["size"]}
        res["same_tree"] = {
            "keep": snapshot(repo, state / "keep", seed="keep") == t1,
            "private_odb": snapshot(repo, state / "priv", private_odb=state / "odb") == t1,
            "guided": snapshot_guided(repo, state / "guided") == t1,
        }

        # diffs between snapshots (T0 = clean, T1 = this scenario)
        res["diff_raw"] = timed(lambda: git(repo, "diff-tree", "-r", "-z", "--raw", "-M", t0, t1), runs=small_runs)
        res["diff_numstat"] = timed(lambda: git(repo, "diff-tree", "-r", "-z", "--numstat", "-M", t0, t1),
                                    runs=small_runs)
        patch_holder = {}

        def full_patch():
            patch_holder["p"] = git(repo, "diff-tree", "-r", "-p", "-M", "--full-index", "--binary", t0, t1).stdout

        res["diff_patch_all"] = timed(full_patch, runs=small_runs)
        res["diff_patch_all"]["bytes"] = len(patch_holder["p"])
        if sc.modify:
            one = sc.modify[0]
            res["diff_one_file"] = timed(lambda: git(repo, "diff", *hunks.DIFF_OPTS, t0, t1, "--", one),
                                         runs=small_runs)

        def snap_then_diff():
            t = snapshot(repo, state / "seed")
            git(repo, "diff-tree", "-r", "-z", "--raw", "-M", t0, t)

        res["snapshot_vs_worktree"] = timed(snap_then_diff, runs=small_runs)
        res["tree_vs_worktree_tracked_only"] = timed(
            lambda: git(repo, "--no-optional-locks", "diff", "--raw", "-z", t0), runs=small_runs)
        res["unstaged_numstat"] = timed(
            lambda: git(repo, "--no-optional-locks", "diff", "--numstat", "-z"), runs=small_runs)

        if sc_name == "small":
            res.update(bench_hunk_ops(repo, sc, small_runs))
        sc.restore()
        out["scenarios"][sc_name] = res
        print(f"  {name}/{sc_name}: seed={res['snap_seed']['median_ms']}ms "
              f"keep={res['snap_keep']['median_ms']}ms status={res['status']['median_ms']}ms "
              f"index_untouched={res['user_index_untouched']}", flush=True)
    return out


def bench_hunk_ops(repo: Path, sc: Scenario, runs: int) -> dict:
    """Stage / unstage / revert one hunk of one modified file."""
    path = sc.modify[0]
    target = sc.orig[path] + b"\n/* sp5 hunk bench */\n"
    (repo / path).write_bytes(target)
    git(repo, "reset", "-q", "--", path)
    fp = hunks.parse(hunks.diff(repo, "--", path))[0]
    patch = fp.hunk_patch(len(fp.hunks) - 1)
    res = {}
    res["hunk_diff_file"] = timed(lambda: hunks.diff(repo, "--", path), runs=runs)
    res["hunk_stage"] = timed(lambda: hunks.apply(repo, patch, "--cached"), runs=runs,
                              setup=lambda i: git(repo, "reset", "-q", "--", path))
    res["hunk_unstage"] = timed(lambda: hunks.apply(repo, patch, "--cached", "-R"), runs=runs,
                                setup=lambda i: (git(repo, "reset", "-q", "--", path),
                                                 hunks.apply(repo, patch, "--cached")))
    git(repo, "reset", "-q", "--", path)
    res["hunk_revert"] = timed(lambda: hunks.apply(repo, patch, "-R"), runs=runs,
                               setup=lambda i: (repo / path).write_bytes(target))
    ok = (repo / path).read_bytes() == sc.orig[path]
    res["hunk_ops_correct"] = ok
    return res


def bench_fsmonitor(name: str, runs: int) -> dict:
    """Same snapshot on the same repo with the builtin fsmonitor + untracked cache."""
    repo = REPOS / name
    state = STATE / (name + "-fsm")
    shutil.rmtree(state, ignore_errors=True)
    out = {"repo": name, "scenarios": {}}
    git(repo, "config", "core.fsmonitor", "true")
    git(repo, "config", "core.untrackedCache", "true")
    try:
        git(repo, "fsmonitor--daemon", "start", check=False)
        time.sleep(2)
        # populate FSMN + UNTR extensions in the real index. Must be -unormal: with -uall git
        # neither uses nor writes the untracked cache.
        for _ in range(3):
            git(repo, "status", "--porcelain=v2")
        for sc_name in ("clean", "small"):
            sc = Scenario(repo, SCENARIOS[sc_name])
            res = {"changes": sc.actual}
            res["status"] = timed(lambda: status(repo), runs=runs, setup=sc.apply)
            res["status_normal"] = timed(lambda: status(repo, "normal"), runs=runs, setup=sc.apply)
            res["snap_seed"] = timed(lambda: snapshot(repo, state / "seed"), runs=runs, setup=sc.apply)
            snapshot(repo, state / "keep")
            res["snap_keep"] = timed(lambda: snapshot(repo, state / "keep", seed="keep"), runs=runs,
                                     setup=sc.apply)
            res["snap_guided"] = timed(lambda: snapshot_guided(repo, state / "guided"), runs=runs,
                                       setup=sc.apply)
            sc.apply(10_001)
            res["guided_same_tree"] = snapshot_guided(repo, state / "guided") == snapshot(repo, state / "seed")
            sc.restore()
            for _ in range(2):
                git(repo, "status", "--porcelain=v2")
            out["scenarios"][sc_name] = res
            print(f"  {name}/fsmonitor/{sc_name}: seed={res['snap_seed']['median_ms']}ms "
                  f"guided={res['snap_guided']['median_ms']}ms status-uall={res['status']['median_ms']}ms "
                  f"status-unormal={res['status_normal']['median_ms']}ms same={res['guided_same_tree']}", flush=True)
    finally:
        git(repo, "fsmonitor--daemon", "stop", check=False)
        git(repo, "config", "core.fsmonitor", "false")
        git(repo, "config", "core.untrackedCache", "false")
        git(repo, "status", "--porcelain", check=False)  # drop the extensions again
        git(repo, "config", "--unset", "core.fsmonitor", check=False)
        git(repo, "config", "--unset", "core.untrackedCache", check=False)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repos", default="syn-1k,syn-10k,syn-100k,linux")
    ap.add_argument("--scenarios", default="clean,small,medium,large")
    ap.add_argument("--runs", type=int, default=5)
    ap.add_argument("--no-fsmonitor", action="store_true")
    ap.add_argument("--fsmonitor-only", action="store_true", help="only the fsmonitor pass (separate result file)")
    a = ap.parse_args()
    out_name = "bench_snapshot_fsmonitor_only" if a.fsmonitor_only else "bench_snapshot"
    results = {"runs": a.runs, "repos": [], "fsmonitor": []}
    if not a.fsmonitor_only:
        for name in a.repos.split(","):
            print(f"[{name}]", flush=True)
            results["repos"].append(bench_repo(name, a.runs, a.scenarios.split(",")))
            save_result(out_name, results)
    if not a.no_fsmonitor:
        for name in a.repos.split(","):
            if name in ("linux", "syn-100k"):
                results["fsmonitor"].append(bench_fsmonitor(name, a.runs))
                save_result(out_name, results)
    save_result(out_name, results)


if __name__ == "__main__":
    sys.exit(main())
