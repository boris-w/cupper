"""Redact local identifiers from experiment output (stdin -> stdout).

Replaces: work/temp/home paths, user name, host name, signing identity hashes,
Team IDs, certificate common names and e-mail addresses.
"""

import getpass
import os
import re
import socket
import subprocess
import sys


def paths():
    out = []
    work = os.environ.get("SP4B_WORK")
    if work:
        work = work.rstrip("/")
        out += [(os.path.realpath(work), "<work>"), (work, "<work>")]
    for var, tag in (("DARWIN_USER_TEMP_DIR", "<tmp>"), ("DARWIN_USER_CACHE_DIR", "<cache>")):
        try:
            p = subprocess.run(["getconf", var], capture_output=True, text=True).stdout.strip().rstrip("/")
        except OSError:
            p = ""
        if p:
            out += [(os.path.realpath(p), tag), (p, tag)]
    tmp = os.environ.get("TMPDIR", "").rstrip("/")
    if tmp:
        out += [(os.path.realpath(tmp), "<tmp>"), (tmp, "<tmp>")]
    home = os.path.expanduser("~")
    out.append((home, "$HOME"))
    # Longest first so that nested paths are replaced before their parents.
    return sorted({p: t for p, t in out if p}.items(), key=lambda x: -len(x[0]))


def main():
    text = sys.stdin.read()
    for p, tag in paths():
        text = text.replace(p, tag)
    for var, tag in (("SP4B_ID_A", "<ID_A>"), ("SP4B_ID_B", "<ID_B>")):
        v = os.environ.get(var)
        if v:
            text = re.sub(re.escape(v), tag, text, flags=re.I)
    for var, tag in (("SP4B_TEAM_A", "<TEAM_A>"), ("SP4B_TEAM_B", "<TEAM_B>")):
        v = os.environ.get(var)
        if v:
            text = text.replace(v, tag)
    # Claude Code encodes cwd paths into project directory names.
    text = re.sub(r"var-folders-[a-z0-9]+-[a-z0-9_]+-[TC]-", "<tmp-encoded>-", text)
    text = re.sub(r"TeamIdentifier=[A-Z0-9]{10}", "TeamIdentifier=<TEAM>", text)
    text = re.sub(r'subject\.OU\] = "?[A-Z0-9]{10}"?', 'subject.OU] = "<TEAM>"', text)
    text = re.sub(r"(Apple Development|Apple Distribution|Developer ID Application|Mac Developer): [^\n\"(]*\([A-Z0-9]{10}\)",
                  r"\1: <NAME> (<ID>)", text)
    text = re.sub(r"[\w.+-]+@[\w-]+\.[\w.-]+", "<email>", text)
    user = getpass.getuser()
    host = socket.gethostname().split(".")[0]
    if user:
        text = re.sub(r"\b%s\b" % re.escape(user), "<user>", text)
    if host:
        text = re.sub(r"\b%s\b" % re.escape(host), "<host>", text)
    sys.stdout.write(text)


main()
