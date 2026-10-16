"""Time worktree create / safety check / remove / rebuild on the sample repos.

  add_b           git worktree add -b cupper/bench-N <path> HEAD     (full checkout)
  add_no_checkout same with --no-checkout (shows the checkout share)
  safety_check    edge_worktree.safety() on a clean worktree (first call right after checkout, then after one index refresh)
  remove          git worktree remove <path>     (clean worktree, no --force)
  rebuild         git worktree add <path> <existing branch>

Usage: python3 bench_worktree.py [--repos syn-1k,syn-10k,syn-100k,linux] [--runs 3]
"""
from __future__ import annotations

import argparse
import shutil
import sys
import time

from edge_worktree import safety
from lib import REPOS, SCRATCH, git, save_result, timed

WT = SCRATCH / "bench-wt"


class Pool:
    def __init__(self, repo, name):
        self.repo, self.name, self.n, self.live = repo, name, 0, []

    def path(self, i):
        return WT / self.name / f"w{i}"

    def branch(self, i):
        return f"cupper/bench-{i}"

    def add(self, *extra):
        i = self.n
        self.n += 1
        git(self.repo, "worktree", "add", "-q", *extra, "-b", self.branch(i), str(self.path(i)), "HEAD")
        self.live.append(i)
        return i

    def drop_all(self, *_):
        for i in self.live:
            git(self.repo, "worktree", "remove", "--force", str(self.path(i)), check=False)
            git(self.repo, "branch", "-D", self.branch(i), check=False)
        self.live = []


def bench(name: str, runs: int) -> dict:
    repo = REPOS / name
    shutil.rmtree(WT / name, ignore_errors=True)
    (WT / name).mkdir(parents=True, exist_ok=True)
    git(repo, "worktree", "prune")
    p = Pool(repo, name)
    out = {"repo": name}
    out["add_b"] = timed(lambda: p.add(), runs=runs, warmup=1, setup=p.drop_all)
    i = p.live[-1]
    # Right after checkout every file has the same mtime second as the new index ("racy git"),
    # so a read-only status re-hashes everything. Measure that once, then the normal case after
    # one index refresh (cupper may write the index of a worktree it created).
    out["safety_check_first"] = timed(lambda: safety(repo, p.path(i), p.branch(i)), runs=1, warmup=0)
    time.sleep(1.1)
    git(p.path(i), "update-index", "-q", "--refresh", check=False)
    out["safety_check"] = timed(lambda: safety(repo, p.path(i), p.branch(i)), runs=runs)
    # remove: a fresh clean worktree is created (untimed) before every run
    out["remove"] = timed(lambda: git(repo, "worktree", "remove", str(p.path(p.live.pop()))), runs=runs,
                          warmup=0, setup=lambda _: (p.drop_all(), p.add()))
    # rebuild from the existing branch: worktree removed (untimed), branch kept
    keep = p.add()
    out["rebuild"] = timed(lambda: git(repo, "worktree", "add", "-q", str(p.path(keep)), p.branch(keep)),
                           runs=runs, warmup=0,
                           setup=lambda _: git(repo, "worktree", "remove", "--force", str(p.path(keep)),
                                               check=False))
    p.drop_all()
    out["add_no_checkout"] = timed(lambda: p.add("--no-checkout"), runs=runs, warmup=1, setup=p.drop_all)
    p.drop_all()
    git(repo, "worktree", "prune")
    for b in git(repo, "for-each-ref", "--format=%(refname:short)", "refs/heads/cupper/bench-*").stdout.split():
        git(repo, "branch", "-D", b.decode())
    print(f"  {name}: add={out['add_b']['median_ms']}ms safety={out['safety_check']['median_ms']}ms "
          f"remove={out['remove']['median_ms']}ms rebuild={out['rebuild']['median_ms']}ms "
          f"no-checkout={out['add_no_checkout']['median_ms']}ms", flush=True)
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repos", default="syn-1k,syn-10k,syn-100k,linux")
    ap.add_argument("--runs", type=int, default=3)
    a = ap.parse_args()
    res = {"runs": a.runs, "repos": []}
    for name in a.repos.split(","):
        res["repos"].append(bench(name, a.runs))
        save_result("bench_worktree", res)
    shutil.rmtree(WT, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
