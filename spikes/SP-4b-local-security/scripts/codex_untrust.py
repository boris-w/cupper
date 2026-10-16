"""Remove the trust entry that `codex exec` writes for a scratch repo.

Usage: python3 -I codex_untrust.py CONFIG_TOML REPO_PATH [EXPECTED_SHA256_PREFIX]

`codex exec` persists `[projects."<cwd>"] trust_level = "trusted"` into the user's config.
This removes exactly that block (for REPO_PATH and its /private realpath), nothing else, and
writes the file back atomically. With EXPECTED_SHA256_PREFIX it also reports whether the result
matches the fingerprint taken before the agent runs. Never prints file content.
"""

import hashlib
import os
import sys

cfg, repo = sys.argv[1], sys.argv[2]
expected = sys.argv[3] if len(sys.argv) > 3 else None
paths = {repo, os.path.realpath(repo)}
if repo.startswith("/var/"):
    paths.add("/private" + repo)
data = open(cfg, "rb").read()
removed = 0
for p in paths:
    # Codex inserts "\n[projects...]\ntrust_level = ...\n" before the next table; drop it as is.
    block = ('\n[projects."%s"]\ntrust_level = "trusted"\n' % p).encode()
    while block in data:
        data = data.replace(block, b"", 1)
        removed += 1
if removed:
    tmp = cfg + ".sp4b-tmp"
    with open(tmp, "wb") as f:
        f.write(data)
    os.chmod(tmp, os.stat(cfg).st_mode & 0o777)
    os.replace(tmp, cfg)
digest = hashlib.sha256(open(cfg, "rb").read()).hexdigest()[:16]
print(f"removed_blocks={removed} sha256_prefix_now={digest}"
      + (f" matches_before={digest == expected}" if expected else ""))
