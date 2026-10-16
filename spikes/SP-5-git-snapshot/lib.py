"""Shared helpers for the SP-5 spike scripts.

Python 3.9+, standard library only. Every git call goes through `git()` so that
the user's global/system git config never leaks into the measurements: we point
GIT_CONFIG_GLOBAL at an empty file and set GIT_CONFIG_NOSYSTEM=1. Edge cases
that depend on a specific config set it explicitly with `-c` or repo-local config.

All test data lives under SP5_WORKDIR (default: $TMPDIR/cupper-sp5). Nothing here
touches any repository outside that directory.
"""
from __future__ import annotations

import getpass
import hashlib
import json
import os
import platform
import statistics
import subprocess
import tempfile
import time
from pathlib import Path
from typing import Callable, Iterable, Optional, Sequence

HERE = Path(__file__).resolve().parent


def _workdir() -> Path:
    w = os.environ.get("SP5_WORKDIR") or os.path.join(tempfile.gettempdir(), "cupper-sp5")
    p = Path(w).expanduser().resolve()
    p.mkdir(parents=True, exist_ok=True)
    return p


WORK = _workdir()
REPOS = WORK / "repos"
RESULTS = WORK / "results"
STATE = WORK / "state"
SCRATCH = WORK / "scratch"
for _d in (REPOS, RESULTS, STATE, SCRATCH):
    _d.mkdir(parents=True, exist_ok=True)

EMPTY_CFG = WORK / "empty.gitconfig"
EMPTY_CFG.touch()
TOOLS_BIN = WORK / "tools" / "bin"  # optional: put a git-lfs binary here

GIT = os.environ.get("SP5_GIT", "git")


class GitError(RuntimeError):
    def __init__(self, args: Sequence[str], proc: subprocess.CompletedProcess):
        self.proc = proc
        self.returncode = proc.returncode
        self.stderr = sanitize(proc.stderr.decode(errors="replace")) if proc.stderr else ""
        super().__init__(sanitize(f"{' '.join(map(str, args))} -> {proc.returncode}: {self.stderr.strip()[:500]}"))


def base_env(extra: Optional[dict] = None) -> dict:
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(
        {
            "GIT_CONFIG_GLOBAL": str(EMPTY_CFG),
            "GIT_CONFIG_NOSYSTEM": "1",
            "GIT_TERMINAL_PROMPT": "0",
            "GIT_ADVICE": "0",
            "GIT_AUTHOR_NAME": "sp5",
            "GIT_AUTHOR_EMAIL": "sp5@example.invalid",
            "GIT_COMMITTER_NAME": "sp5",
            "GIT_COMMITTER_EMAIL": "sp5@example.invalid",
        }
    )
    if TOOLS_BIN.is_dir():
        env["PATH"] = f"{TOOLS_BIN}{os.pathsep}{env.get('PATH', '')}"
    if extra:
        for k, v in extra.items():
            if v is None:
                env.pop(k, None)
            else:
                env[k] = str(v)
    return env


def run(args: Sequence, cwd=None, env: Optional[dict] = None, input: Optional[bytes] = None,
        check: bool = True) -> subprocess.CompletedProcess:
    args = [str(a) for a in args]
    proc = subprocess.run(args, cwd=cwd, env=env if env is not None else base_env(), input=input,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if check and proc.returncode != 0:
        raise GitError(args, proc)
    return proc


def git(repo, *args, env: Optional[dict] = None, input: Optional[bytes] = None,
        check: bool = True) -> subprocess.CompletedProcess:
    return run([GIT, *args], cwd=repo, env=base_env(env), input=input, check=check)


def git_out(repo, *args, **kw) -> str:
    return git(repo, *args, **kw).stdout.decode().strip()


def git_path(repo, name: str) -> Path:
    """Absolute path of a file inside $GIT_DIR (works for linked worktrees)."""
    return Path(git_out(repo, "rev-parse", "--path-format=absolute", "--git-path", name))


def timed(fn: Callable[[], object], runs: int = 5, warmup: int = 1,
          setup: Optional[Callable[[int], object]] = None) -> dict:
    """Run fn `warmup` + `runs` times; setup(i) runs before each call, untimed."""
    i = 0
    for _ in range(warmup):
        if setup:
            setup(i)
        i += 1
        fn()
    samples = []
    for _ in range(runs):
        if setup:
            setup(i)
        i += 1
        t = time.perf_counter()
        fn()
        samples.append((time.perf_counter() - t) * 1000.0)
    return {
        "median_ms": round(statistics.median(samples), 1),
        "min_ms": round(min(samples), 1),
        "max_ms": round(max(samples), 1),
        "runs": runs,
    }


def sha256_file(p) -> Optional[str]:
    p = Path(p)
    if not p.exists():
        return None
    h = hashlib.sha256()
    with open(p, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def file_state(p) -> Optional[tuple]:
    """(sha256, mtime_ns) of a file, used to prove a file was not touched."""
    p = Path(p)
    if not p.exists():
        return None
    return (sha256_file(p), p.stat().st_mtime_ns)


def _cmd(args: Sequence[str]) -> str:
    try:
        return subprocess.run(list(args), stdout=subprocess.PIPE, stderr=subprocess.DEVNULL).stdout.decode().strip()
    except OSError:
        return ""


def env_info() -> dict:
    info = {
        "git": git_out(WORK, "--version"),
        "os": f"macOS {_cmd(['sw_vers', '-productVersion'])} ({_cmd(['sw_vers', '-buildVersion'])})"
        if platform.system() == "Darwin" else platform.platform(),
        "cpu": _cmd(["sysctl", "-n", "machdep.cpu.brand_string"]) or platform.processor(),
        "cores": os.cpu_count(),
        "mem_gb": round(int(_cmd(["sysctl", "-n", "hw.memsize"]) or 0) / 2**30),
        "python": platform.python_version(),
    }
    dev = _cmd(["df", str(WORK)]).splitlines()[-1].split()[0] if _cmd(["df", str(WORK)]) else ""
    disk = _cmd(["diskutil", "info", dev]) if dev else ""
    fields = {}
    for line in disk.splitlines():
        if ":" in line:
            k, v = line.split(":", 1)
            fields[k.strip()] = v.strip()
    info["disk"] = ", ".join(
        x for x in [
            "SSD" if fields.get("Solid State") == "Yes" else fields.get("Solid State", ""),
            fields.get("Protocol", ""),
            fields.get("File System Personality", ""),
        ] if x
    )
    lfs = run(["git-lfs", "version"], env=base_env(), check=False)
    info["git_lfs"] = lfs.stdout.decode().strip() if lfs.returncode == 0 else "not available"
    return info


def sanitize(s: str) -> str:
    """Strip machine-specific paths and the user name before anything is saved."""
    reps = []
    for p in {str(WORK), os.path.realpath(str(WORK))}:
        reps.append((p, "<tmp>"))
        if p.startswith("/private/"):  # macOS: /tmp and /var are symlinks into /private
            reps.append((p[len("/private"):], "<tmp>"))
    home = str(Path.home())
    reps.append((home, "$HOME"))
    reps.sort(key=lambda x: -len(x[0]))
    for a, b in reps:
        s = s.replace(a, b)
    user = getpass.getuser()
    if user and len(user) > 2:
        s = s.replace(user, "<user>")
    return s


def err_text(stderr, n: int = 140, last: bool = False) -> str:
    """One sanitized line of stderr. Sanitize BEFORE truncating, or partial paths leak."""
    if isinstance(stderr, (bytes, bytearray)):
        stderr = stderr.decode(errors="replace")
    lines = [l for l in sanitize(stderr).strip().splitlines() if l.strip()]
    if not lines:
        return ""
    return (lines[-1] if last else lines[0])[:n]


def save_result(name: str, data) -> Path:
    path = RESULTS / f"{name}.json"
    text = sanitize(json.dumps(data, indent=2, ensure_ascii=False))
    path.write_text(text + "\n")
    return path


def load_result(name: str):
    path = RESULTS / f"{name}.json"
    return json.loads(path.read_text()) if path.exists() else None


def md_table(headers: Sequence[str], rows: Iterable[Sequence]) -> str:
    out = ["| " + " | ".join(headers) + " |", "|" + "---|" * len(headers)]
    for r in rows:
        out.append("| " + " | ".join("" if c is None else str(c) for c in r) + " |")
    return "\n".join(out)


def write_bytes(path, data: bytes, mode: Optional[int] = None) -> None:
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
    if mode is not None:
        os.chmod(path, mode)


def new_repo(name: str, *, bare: bool = False, initial_branch: str = "main") -> Path:
    """Fresh empty repo under SCRATCH/<name> (recreated each call)."""
    path = SCRATCH / name
    if path.exists():
        _force_rmtree(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    args = ["init", "-q", f"--initial-branch={initial_branch}"]
    if bare:
        args.append("--bare")
    git(path.parent, *args, str(path))
    return path


def _force_rmtree(path: Path) -> None:
    import shutil
    import stat

    def onerror(func, p, _exc):
        try:
            os.chmod(p, stat.S_IRWXU)
            func(p)
        except OSError:
            pass

    # make everything writable/readable first (edge tests chmod 000 some files)
    for root, dirs, files in os.walk(path):
        for n in dirs + files:
            try:
                os.chmod(os.path.join(root, n), stat.S_IRWXU, follow_symlinks=False)
            except (OSError, NotImplementedError):
                pass
    shutil.rmtree(path, onerror=onerror)


def commit_all(repo, msg: str = "c") -> str:
    git(repo, "add", "-A")
    git(repo, "commit", "-q", "--no-verify", "-m", msg)
    return git_out(repo, "rev-parse", "HEAD")
