"""Turn-snapshot methods under test.

A snapshot is a git tree id describing the full working-tree state (tracked +
untracked, .gitignore respected) without touching the user's index, HEAD, refs
or working tree.

Method A ("temp index"):
    cp <git-dir>/index  <state>/snap.index         # seed: reuse the stat cache
    GIT_INDEX_FILE=<state>/snap.index git add -A
    GIT_INDEX_FILE=<state>/snap.index git write-tree
Variants:
    seed="index"  copy the user's index every time (default)
    seed="empty"  start from an empty temp index (forces hashing every file)
    seed="keep"   keep the temp index from the previous snapshot of this session
    private_odb   write new blobs/trees into a cupper-owned object dir and read
                  the repo's objects through GIT_ALTERNATE_OBJECT_DIRECTORIES,
                  so the user's .git/objects is never written either.

Method B: `git stash create` (for comparison only; see results).
"""
from __future__ import annotations

import os
import shutil
from pathlib import Path
from typing import Iterable, Optional

from lib import git, git_out, git_path

# Config overrides that make `git add -A` safe to run as a background snapshot.
SNAPSHOT_CFG = [
    "-c", "advice.addEmbeddedRepo=false",
    "-c", "core.safecrlf=false",       # a user's safecrlf=true would abort add on mixed EOLs
    "-c", "add.ignoreErrors=false",    # we pass --ignore-errors explicitly when wanted
]


def odb_env(repo, private_odb: Optional[Path]) -> dict:
    if not private_odb:
        return {}
    private_odb = Path(private_odb)
    (private_odb / "info").mkdir(parents=True, exist_ok=True)
    (private_odb / "pack").mkdir(parents=True, exist_ok=True)
    repo_objects = git_path(repo, "objects")
    alts = [str(repo_objects)]
    alt_file = repo_objects / "info" / "alternates"
    if alt_file.exists():  # keep the repo's own alternates reachable too
        for line in alt_file.read_text().splitlines():
            line = line.strip()
            if line and not line.startswith("#"):
                p = Path(line)
                alts.append(str(p if p.is_absolute() else (repo_objects / p).resolve()))
    return {
        "GIT_OBJECT_DIRECTORY": str(private_odb),
        "GIT_ALTERNATE_OBJECT_DIRECTORIES": os.pathsep.join(alts),
    }


def snapshot(repo, state_dir, *, seed: str = "index", private_odb: Optional[Path] = None,
             ignore_errors: bool = False, exclude: Iterable[str] = (), extra_cfg: Iterable[str] = (),
             env: Optional[dict] = None) -> str:
    state_dir = Path(state_dir)
    state_dir.mkdir(parents=True, exist_ok=True)
    tmp_index = state_dir / "snap.index"
    if seed == "index":
        real = git_path(repo, "index")
        if real.exists():
            shutil.copyfile(real, tmp_index)
        elif tmp_index.exists():
            tmp_index.unlink()
    elif seed == "empty":
        if tmp_index.exists():
            tmp_index.unlink()
    elif seed == "keep":
        if not tmp_index.exists():
            real = git_path(repo, "index")
            if real.exists():
                shutil.copyfile(real, tmp_index)
    else:
        raise ValueError(seed)

    e = {"GIT_INDEX_FILE": str(tmp_index)}
    e.update(odb_env(repo, private_odb))
    if env:
        e.update(env)
    add = [*SNAPSHOT_CFG, *extra_cfg, "add", "-A"]
    if ignore_errors:
        add.append("--ignore-errors")
    exclude = list(exclude)
    if exclude:
        # many paths -> use a pathspec file; ':/' = whole tree from the top
        spec = state_dir / "snap.pathspec"
        spec.write_bytes(b"\0".join([b":/"] + [f":(top,literal,exclude){p}".encode() for p in exclude]) + b"\0")
        add += [f"--pathspec-from-file={spec}", "--pathspec-file-nul"]
    git(repo, *add, env=e)
    return git_out(repo, "write-tree", env=e)


def anchor(repo, tree: str, ref: str, *, parent: Optional[str] = "HEAD", message: str = "cupper snapshot",
           private_odb: Optional[Path] = None) -> str:
    """Optionally wrap a snapshot tree in a commit and point a private ref at it."""
    e = odb_env(repo, private_odb)
    args = ["commit-tree", tree, "-m", message]
    if parent:
        args[2:2] = ["-p", parent]
    commit = git_out(repo, *args, env=e)
    git(repo, "update-ref", ref, commit, env=e)
    return commit


def stash_create(repo) -> str:
    """Method B: returns a commit (or '' when there are no tracked changes)."""
    return git_out(repo, "stash", "create")


def diff_env(repo, private_odb: Optional[Path]) -> dict:
    return odb_env(repo, private_odb)
