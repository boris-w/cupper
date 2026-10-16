"""Alternative snapshot design: a separate "shadow" repository.

  GIT_DIR=<state>/shadow.git GIT_WORK_TREE=<repo> git add -A && git write-tree

The shadow repo has its own index and object store, so nothing in the user's .git is
touched - but it does not see the repo's .git/info/exclude or config (filters, autocrlf,
LFS), and the first snapshot has to hash AND store every file.

Usage: python3 bench_shadow.py [--repos syn-10k,linux] [--runs 3]
"""
from __future__ import annotations

import argparse
import shutil
import sys
import time

from bench_snapshot import SCENARIOS, Scenario
from lib import REPOS, STATE, git, git_out, save_result, timed
from snapshot import snapshot


def du_mb(path) -> int:
    return round(sum(p.stat().st_size for p in path.rglob("*") if p.is_file()) / 2**20)


def bench(name: str, runs: int) -> dict:
    repo = REPOS / name
    shadow = STATE / f"shadow-{name}.git"
    shutil.rmtree(shadow, ignore_errors=True)
    git(STATE, "init", "-q", "--bare", str(shadow))
    env = {"GIT_DIR": str(shadow), "GIT_WORK_TREE": str(repo)}

    def snap():
        git(repo, "-c", "advice.addEmbeddedRepo=false", "add", "-A", env=env)
        return git_out(repo, "write-tree", env=env)

    out = {"repo": name}
    t = time.perf_counter()
    first = snap()
    out["first_snapshot_ms"] = round((time.perf_counter() - t) * 1000)
    out["shadow_store_mb"] = du_mb(shadow)
    ref = snapshot(repo, STATE / f"shadow-cmp-{name}")
    out["same_tree_as_temp_index"] = first == ref
    if first != ref:  # both trees are in different object stores; compare path lists
        a = set(git(repo, "ls-tree", "-r", "--name-only", "-z", first, env=env).stdout.split(b"\0"))
        b = set(git(repo, "ls-tree", "-r", "--name-only", "-z", ref).stdout.split(b"\0"))
        out["only_in_temp_index"] = sorted(p.decode(errors="replace") for p in b - a)[:20]
        out["only_in_shadow"] = sorted(p.decode(errors="replace") for p in a - b)[:20]
    out["next_clean"] = timed(snap, runs=runs)
    sc = Scenario(repo, SCENARIOS["small"])
    out["next_small"] = timed(snap, runs=runs, setup=sc.apply)
    sc.restore()
    print(f"  {name}: first={out['first_snapshot_ms']}ms store={out['shadow_store_mb']}MB "
          f"next_clean={out['next_clean']['median_ms']}ms next_small={out['next_small']['median_ms']}ms "
          f"same_tree={out['same_tree_as_temp_index']}", flush=True)
    shutil.rmtree(shadow, ignore_errors=True)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repos", default="syn-10k,linux")
    ap.add_argument("--runs", type=int, default=3)
    a = ap.parse_args()
    res = {"repos": [bench(n, a.runs) for n in a.repos.split(",")]}
    save_result("bench_shadow", res)


if __name__ == "__main__":
    sys.exit(main())
