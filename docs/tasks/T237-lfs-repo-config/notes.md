# T237 notes

## The audit

Pinned git-lfs 3.8.0, Apple Git 2.50.1. The keys come from the release's
`git-lfs-config(5)`. The repo tier was emulated with plain git: the
standard LFS filters given the host's git-lfs by its absolute path,
`core.hooksPath` on an empty directory, and the keys `names_program` covers
emptied, as `rules::overrides` does. Programs were marker scripts. The
"server" was a local HTTP endpoint that records each request (401 for
`/auth` paths, 404 otherwise). The smudge was `checkout -- a.bin` or a
`worktree add`, with the object removed from `.git/lfs/objects` where a
download was the point. The clean was `update-index --refresh` of a changed
file.

### Programs

| key | where | runs |
|---|---|---|
| `lfs.extension.<n>.clean` | `.git/config` | on every clean (marker made by a refresh) |
| `lfs.extension.<n>.smudge` | `.git/config` | on a smudge of a pointer that names the extension |
| `lfs.extension.*` | `.lfsconfig` | no: "unsafe '.lfsconfig' keys were ignored"; the clean then fails |
| `lfs.customtransfer.<n>.path` + `lfs.standalonetransferagent=<n>` | `.git/config` | on a download, without asking any server (16 runs, its retries) |
| the same | `.lfsconfig` | no: ignored as unsafe |
| `lfs.customtransfer.<n>.path` alone | `.git/config` | only if a server's batch response picks it (a 404 server: no) |
| `credential.helper` | `.git/config` | on a 401, unless emptied; the repo tier empties it: not run |
| `core.askpass` | `.git/config` | emptied by the repo tier: not run |
| `ssh://` endpoint, `core.sshCommand` emptied | any | `ssh` from PATH: `ssh -oControlMaster=yes … git-lfs-transfer`, then `git-lfs-authenticate` (`lfs.ssh.automultiplex`) |
| `lfs.url=cupper-offline:` (no scheme) | `-c` | read as an ssh host: ssh connected through the machine's proxy |
| `file://` endpoint | any | `git-lfs standalone-file` looked up on PATH ("git-lfs: command not found" without one) |
| git-lfs's own hooks | the hooks directory | see below |

`-c lfs.extension.x.clean=` does not disarm an extension: git-lfs fails
instead.

### git-lfs writes hooks into cupper's empty hooks directory

Running as a filter, git-lfs installs its `pre-push`, `post-checkout`,
`post-commit` and `post-merge` hooks where `core.hooksPath` points: cupper's
`<data dir>/git/empty-hooks/`. This happens on a clean (a refresh), on a smudge
(`apply -R`, `checkout`) and in a `worktree add`. The snapshot tier does the
same: after `it_git_lfs_snapshot_runs_the_hosts_git_lfs`, the data
directory's `empty-hooks` held the four hooks. From then on every repo tier
command that runs a hook runs git-lfs's script. `worktree add` runs
`post-checkout`, which runs `git lfs post-checkout` if `git-lfs` is on the
trusted PATH, and otherwise exits 2, failing the `worktree add` after the
checkout. Seen as "This repository is configured for Git LFS but 'git-lfs'
was not found on your path", exit 2, the worktree left behind.

A hooks path git-lfs cannot write stops it, and its filters still work:
- a read-only directory (0500): no hooks written, the smudge went on;
- `/dev/null`: no hooks written, the smudge went on.

### The network

git-lfs goes to the network only to download an object that is not
local. With the object present, endpoints in `.git/config` and
`.lfsconfig`, rewrites and proxies made no request; a clean made none.

| source of the endpoint | where | request on a missing object |
|---|---|---|
| `lfs.url` | `.git/config` | `POST /x/objects/batch` |
| `lfs.url` | `.lfsconfig` (committed; also read from `:.lfsconfig` and `HEAD:.lfsconfig`) | yes, in place and in a `worktree add` |
| `remote.origin.lfsurl` | `.lfsconfig` | yes |
| `remote.origin.url` | `.git/config` | `POST /repo.git/info/lfs/objects/batch` |
| `url.<base>.insteadOf` | `.git/config` | rewrites `lfs.url`: the request goes to the rewritten URL |
| `http.proxy` | `.git/config` | the request goes to the proxy |
| none | | no request: "missing protocol" |

`.lfsconfig` is limited to `lfs.allowincompletepush`, `lfs.fetchexclude`,
`lfs.fetchinclude`, `lfs.gitprotocol`, `lfs.locksverify`, `lfs.pushurl`,
`lfs.skipdownloaderrors`, `lfs.url`, `lfs.*.access` and `remote.*.lfsurl`,
but an agent can commit one.

Overriding the endpoint does not keep git-lfs offline. `url.*.insteadOf`
rewrites whatever URL is set, and a value without a scheme is taken for an
ssh host.

### Smudges that write the pointer instead

| setting | effect on a smudge |
|---|---|
| `lfs.fetchexclude=*` in `.lfsconfig` | the pointer is written into the work tree, object present or not, exit 0 |
| `-c lfs.fetchexclude=` over it | the content |
| `lfs.skipdownloaderrors=true` in `.lfsconfig` | after the failed download, the pointer is written |
| `GIT_LFS_SKIP_SMUDGE=1` | the pointer, object present or not |

So neither `GIT_LFS_SKIP_SMUDGE` nor `lfs.fetchexclude` is an offline smudge:
both write pointers for objects that are present.

## The snapshot tier

Its git-lfs reads cupper's GIT_DIR configuration (the whitelist), an empty
global one and no system one. No `lfs.extension.*` or `lfs.customtransfer.*`
reaches it. From `.lfsconfig` it takes only the keys above. It only cleans,
and a clean neither downloads nor runs a transfer agent. What it does do is
write the hooks above.

## Before the fix: revert

T235's IT, with the copy skipped, showed a revert's smudge going for the
object (`Downloading a.bin`, `batch request: missing protocol`). The
failure came back as `conflict/stale_diff`.
