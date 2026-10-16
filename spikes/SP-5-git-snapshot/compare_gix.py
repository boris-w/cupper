"""gitoxide (gix 0.89) vs git CLI, through the small probe binary in gix-probe/.

  gix_status    full status (HEAD-index + index-worktree + every untracked file)
  gix_snapshot  emulated snapshot (gix has no `add -A`): status -> write blobs -> edit HEAD tree
                -> tree id compared with the CLI snapshot of the same state
  gix_diff      tree-to-tree diff (paths only)
  gix_wtadd     Repository::add_worktree() on a new branch
  compat        status on the special repos made by edge_snapshot.py

Needs cargo. The binary is built into $SP5_WORKDIR/cargo-target.
Usage: python3 compare_gix.py [--repos syn-1k,syn-10k,syn-100k,linux] [--runs 5]
"""
from __future__ import annotations

import argparse
import os
import shutil
import sys

from bench_snapshot import SCENARIOS, Scenario
from lib import HERE, REPOS, SCRATCH, STATE, WORK, err_text, git, run, save_result, timed
from snapshot import snapshot

TARGET = WORK / "cargo-target"
BIN = TARGET / "release" / "sp5-gix-probe"


def build() -> str:
    env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET))
    run(["cargo", "build", "--release", "--quiet", "--manifest-path", str(HERE / "gix-probe" / "Cargo.toml")],
        env=env)
    meta = run(["cargo", "tree", "--depth", "1", "--prefix", "none", "--manifest-path",
                str(HERE / "gix-probe" / "Cargo.toml")], env=env).stdout.decode()
    return next((l.strip() for l in meta.splitlines() if l.startswith("gix ")), "gix ?")


def probe(*args) -> str:
    return run([str(BIN), *map(str, args)]).stdout.decode().strip()


def bench_repo(name: str, runs: int) -> dict:
    repo = REPOS / name
    state = STATE / f"gix-{name}"
    shutil.rmtree(state, ignore_errors=True)
    git(repo, "update-index", "-q", "--refresh", check=False)
    out = {"repo": name, "scenarios": {}}
    t0 = snapshot(repo, state / "t0")
    for sc_name in ("clean", "small", "medium"):
        sc = Scenario(repo, SCENARIOS[sc_name])
        res = {"changes": sc.actual}
        res["gix_status"] = timed(lambda: probe("status", repo), runs=runs, setup=sc.apply)
        res["gix_snapshot"] = timed(lambda: probe("snapshot", repo), runs=runs, setup=sc.apply)
        sc.apply(4243)
        t_gix = probe("snapshot", repo)
        t_cli = snapshot(repo, state / "cmp")
        res["same_tree_as_cli"] = t_gix == t_cli
        if t_gix != t_cli:
            res["tree_diff_vs_cli"] = git(repo, "diff", "--stat", t_cli, t_gix).stdout.decode()[-300:]
        res["gix_diff"] = timed(lambda: probe("diff", repo, t0, t_cli), runs=runs)
        res["cli_diff_raw"] = timed(lambda: git(repo, "diff-tree", "-r", "-z", "--raw", "-M", t0, t_cli), runs=runs)
        sc.restore()
        out["scenarios"][sc_name] = res
        print(f"  {name}/{sc_name}: gix status={res['gix_status']['median_ms']}ms "
              f"snapshot={res['gix_snapshot']['median_ms']}ms same_tree={res['same_tree_as_cli']}", flush=True)
    # worktree add through gitoxide
    wt = SCRATCH / "gix-wt"
    shutil.rmtree(wt, ignore_errors=True)
    wt.mkdir(parents=True)
    n = {"i": 0}

    def cleanup(_=None):
        for i in range(n["i"]):
            git(repo, "worktree", "remove", "--force", str(wt / f"{name}-{i}"), check=False)
            git(repo, "branch", "-D", f"cupper/gix-{i}", check=False)
        git(repo, "worktree", "prune")

    def add():
        i = n["i"]
        n["i"] += 1
        git(repo, "branch", f"cupper/gix-{i}", "HEAD")
        probe("wtadd", repo, f"cupper/gix-{i}", wt / f"{name}-{i}")

    big = name in ("linux", "syn-100k")
    out["gix_wtadd"] = timed(add, runs=3 if big else runs, warmup=1, setup=cleanup)
    last = wt / f"{name}-{n['i'] - 1}"
    st = git(last, "--no-optional-locks", "status", "--porcelain").stdout.decode()
    out["gix_worktree_status_by_cli"] = st[:200]
    cleanup()
    return out


def compat() -> list:
    res = []
    for label, path in [("split index", SCRATCH / "snap-split"), ("sparse checkout", SCRATCH / "snap-sparse"),
                        ("relative worktrees ext", SCRATCH / "wt-relative")]:
        if not path.exists():
            res.append({"case": label, "result": "skipped"})
            continue
        p = run([str(BIN), "status", str(path)], check=False)
        res.append({"case": label, "result": "ok" if p.returncode == 0 else "ERROR",
                    "detail": p.stdout.decode().strip() if p.returncode == 0 else err_text(p.stderr, 160)})
    return res


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--repos", default="syn-1k,syn-10k,syn-100k,linux")
    ap.add_argument("--runs", type=int, default=5)
    a = ap.parse_args()
    res = {"version": build(), "repos": []}
    print(res["version"])
    for name in a.repos.split(","):
        res["repos"].append(bench_repo(name, a.runs))
        save_result("compare_gix", res)
    res["compat"] = compat()
    for c in res["compat"]:
        print("compat:", c)
    save_result("compare_gix", res)


if __name__ == "__main__":
    sys.exit(main())
