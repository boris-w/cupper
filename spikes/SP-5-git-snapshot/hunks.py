"""Split `git diff` output into per-file, per-hunk patches.

Works on bytes end to end, so CRLF, invalid UTF-8 and "\\ No newline at end of
file" markers survive unchanged. The single-hunk patch is the file header plus
one hunk; `git apply` locates it by the old-side line numbers and tolerates the
offset caused by other hunks that were not applied.

Header policy (see results):
- "old mode"/"new mode" lines are stripped from content hunks and offered as a
  separate mode-only pseudo hunk, otherwise the second hunk of a file whose mode
  also changed would no longer match.
- new/deleted/binary/rename-without-content files have exactly one "hunk": the
  whole file patch.
"""
from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import List, Optional

from lib import git

HUNK_RE = re.compile(rb"^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@")

# Options that pin down diff output regardless of the user's diff.* config.
DIFF_OPTS = [
    "--no-color", "--no-ext-diff", "--no-textconv", "--src-prefix=a/", "--dst-prefix=b/",
    "--full-index", "--binary", "-U3",
]


@dataclass
class Hunk:
    header: bytes                 # "@@ -a,b +c,d @@ ..." line incl. newline
    lines: List[bytes]            # body lines incl. newline (and "\ No newline" markers)

    @property
    def old_start(self) -> int:
        return int(HUNK_RE.match(self.header).group(1))

    @property
    def new_start(self) -> int:
        return int(HUNK_RE.match(self.header).group(3))


@dataclass
class FilePatch:
    header: List[bytes]
    hunks: List[Hunk] = field(default_factory=list)
    binary: List[bytes] = field(default_factory=list)   # "GIT binary patch" block or "Binary files ... differ"

    def has(self, prefix: bytes) -> bool:
        return any(h.startswith(prefix) for h in self.header)

    @property
    def path(self) -> bytes:
        return self.header[0].rstrip(b"\n")

    @property
    def is_new(self) -> bool:
        return self.has(b"new file mode")

    @property
    def is_deleted(self) -> bool:
        return self.has(b"deleted file mode")

    @property
    def is_rename(self) -> bool:
        return self.has(b"rename from")

    @property
    def mode_change(self) -> bool:
        return self.has(b"old mode")

    @property
    def is_binary(self) -> bool:
        return bool(self.binary)

    def whole(self) -> bytes:
        out = b"".join(self.header)
        for h in self.hunks:
            out += h.header + b"".join(h.lines)
        out += b"".join(self.binary)
        return out

    def units(self) -> List[str]:
        """What the review UI can act on for this file."""
        if self.is_new or self.is_deleted or self.is_binary or (self.is_rename and not self.hunks):
            return ["whole"]
        u = [f"hunk{i}" for i in range(len(self.hunks))]
        if self.mode_change:
            u.append("mode")
        return u

    def hunk_patch(self, i: int, keep_mode: bool = False) -> bytes:
        if self.is_new or self.is_deleted or self.is_binary:
            return self.whole()
        hdr = [h for h in self.header
               if keep_mode or not (h.startswith(b"old mode") or h.startswith(b"new mode"))]
        h = self.hunks[i]
        return b"".join(hdr) + h.header + b"".join(h.lines)

    def mode_patch(self) -> bytes:
        """Only the mode change: 'diff --git' + old/new mode lines."""
        keep = [h for h in self.header if h.startswith((b"diff --git", b"old mode", b"new mode"))]
        return b"".join(keep)


def normalize_new_start(patch: bytes) -> bytes:
    """Make a single-hunk patch self-consistent: new_start = old_start.

    `git diff` numbers the new side assuming every earlier hunk is applied too. git apply
    ignores that and searches around old_start, but libgit2 applies a hunk exactly at
    new_start, so an extracted hunk after an unapplied one misses. (Needed for libgit2 only.)
    """
    out = []
    for line in patch.splitlines(keepends=True):
        m = HUNK_RE.match(line)
        if m and m.group(2) != b"0" and m.group(1) != b"0":
            old_start, old_n, new_n = m.group(1), m.group(2), m.group(4)
            rest = line[m.end():]
            line = b"@@ -" + old_start + (b"," + old_n if old_n is not None else b"") + \
                b" +" + old_start + (b"," + new_n if new_n is not None else b"") + b" @@" + rest
        out.append(line)
    return b"".join(out)


def parse(patch: bytes) -> List[FilePatch]:
    files: List[FilePatch] = []
    cur: Optional[FilePatch] = None
    hunk: Optional[Hunk] = None
    in_binary = False
    for line in patch.splitlines(keepends=True):
        if line.startswith(b"diff --git "):
            cur = FilePatch(header=[line])
            files.append(cur)
            hunk = None
            in_binary = False
            continue
        if cur is None:
            continue
        if in_binary:
            cur.binary.append(line)
            continue
        if line.startswith(b"@@ "):
            hunk = Hunk(header=line, lines=[])
            cur.hunks.append(hunk)
            continue
        if hunk is not None:
            hunk.lines.append(line)
            continue
        if line.startswith(b"GIT binary patch") or line.startswith(b"Binary files "):
            in_binary = True
            cur.binary.append(line)
            continue
        cur.header.append(line)
    return files


def diff(repo, *args, env=None) -> bytes:
    return git(repo, "diff", *DIFF_OPTS, *args, env=env).stdout


def apply(repo, patch: bytes, *args, env=None, check=True):
    """git apply with options that make it deterministic for hunk operations."""
    return git(repo, "apply", "--whitespace=nowarn", *args, "-", input=patch, env=env, check=check)
