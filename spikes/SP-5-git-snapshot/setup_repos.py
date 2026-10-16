"""Prepare the sample repositories under $SP5_WORKDIR/repos.

- linux:    shallow clone (--depth 1) of torvalds/linux, ~96k files. Only git
            commands are used on it; nothing inside the clone is executed.
- syn-1k / syn-10k / syn-100k: synthetic repos with N small text files, used to
  see how cost scales with file count.

Usage: python3 setup_repos.py [--skip-linux] [--linux-url URL] [--sizes 1000,10000,100000]
"""
from __future__ import annotations

import argparse
import random
import sys

from lib import REPOS, env_info, git, git_out, save_result

LINUX_URL = "https://github.com/torvalds/linux.git"


def text_file(rng: random.Random, n: int, lines: int = 40) -> bytes:
    words = ["alpha", "beta", "gamma", "delta", "kappa", "omega", "sigma", "theta", "zeta", "lambda"]
    out = [f"// synthetic file {n}\n"]
    for i in range(lines):
        out.append(f"line {i:03d} " + " ".join(rng.choice(words) for _ in range(8)) + "\n")
    return "".join(out).encode()


def make_synthetic(n_files: int) -> None:
    name = f"syn-{n_files // 1000}k"
    path = REPOS / name
    if (path / ".git").exists():
        print(f"{name}: exists")
        return
    path.mkdir(parents=True, exist_ok=True)
    git(path, "init", "-q", "--initial-branch=main")
    rng = random.Random(n_files)
    per_dir = 50
    for n in range(n_files):
        d = path / f"d{n // (per_dir * 40):03d}" / f"s{(n // per_dir) % 40:02d}"
        d.mkdir(parents=True, exist_ok=True)
        (d / f"f{n:06d}.txt").write_bytes(text_file(rng, n))
    (path / ".gitignore").write_text("build/\n*.o\n")
    git(path, "add", "-A")
    git(path, "commit", "-q", "--no-verify", "-m", f"synthetic {n_files} files")
    print(f"{name}: created")


def clone_linux(url: str) -> None:
    path = REPOS / "linux"
    if (path / ".git").exists():
        print("linux: exists")
        return
    print("linux: cloning (shallow) ...", flush=True)
    git(REPOS, "clone", "--depth", "1", "--single-branch", "--no-tags", url, str(path))


def describe(name: str) -> dict:
    path = REPOS / name
    files = git(path, "ls-files", "-z").stdout.count(b"\0")
    return {
        "name": name,
        "files": files,
        "head": git_out(path, "rev-parse", "--short=12", "HEAD"),
        "index_bytes": (path / ".git" / "index").stat().st_size,
        "tracked_mb": tracked_mb(path),
    }


def tracked_mb(path) -> int:
    sizes = git_out(path, "ls-tree", "-r", "HEAD", "--format=%(objectsize)").split()
    return round(sum(int(x) for x in sizes if x.isdigit()) / 2**20)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--skip-linux", action="store_true")
    ap.add_argument("--linux-url", default=LINUX_URL)
    ap.add_argument("--sizes", default="1000,10000,100000")
    a = ap.parse_args()
    sizes = [int(x) for x in a.sizes.split(",") if x]
    for n in sizes:
        make_synthetic(n)
    if not a.skip_linux:
        clone_linux(a.linux_url)
    repos = [f"syn-{n // 1000}k" for n in sizes] + ([] if a.skip_linux else ["linux"])
    info = {"env": env_info(), "repos": [describe(r) for r in repos]}
    save_result("env", info)
    for r in info["repos"]:
        print(r)
    print(info["env"])


if __name__ == "__main__":
    sys.exit(main())
