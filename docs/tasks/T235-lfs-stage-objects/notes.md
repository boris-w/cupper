# T235 notes

## Reproduction

On T229's LFS write case (pinned git-lfs 3.8.0): the repo tier ran
`apply --cached` of the unstaged patch (a.bin `one` to `two`), the work
tree then got `three`, and the user committed:

```
objects: openError: a.bin (27dd8ed4…0a5a) could not be checked: no such file or directory
status=1
```

The user's `.git/lfs/objects` held `one` and `three` (the commit's index
refresh cleaned `three`); `two`, the committed pointer's object, was only in
`<data dir>/git/<repo>/ws/<work tree>/lfs/objects/27/dd/`.

## Where git-lfs keeps objects

`git lfs env` of the pinned 3.8.0:

| setting | `LocalMediaDir` |
|---|---|
| none | `<common dir>/lfs/objects`, a linked worktree's too |
| `lfs.storage = store-rel` (git config) | `<common dir>/store-rel/objects`, from a linked worktree too |
| `lfs.storage` in `.lfsconfig` only | `<common dir>/lfs/objects`: ignored |
| `lfs.storage = ~/abs-home` | `<common dir>/~/abs-home/objects`: no `~` expansion |

The temporary directory is `<storage>/tmp`.

## The fix

Before a repo tier write (after the repository's lock, before the command
is `pending_external`), the oids of the pointers the write puts in place
are taken from the patch: the `+oid sha256:` lines when staging, the `-`
ones for unstaging and reverting, which apply the patch reversed. Each the
user's store lacks and cupper's has is copied to `<storage>/tmp/`, checked
against its oid, synced and renamed into place. Files and the directories
made get `core.sharedRepository`'s mode as git reads the value (`group`,
`all`, an octal mode, a boolean), directories an `x` for each `r`, as
git-lfs makes them; without it, the umask's. A copy that fails, or does not
hash to its oid, fails the call with `git_error` before git runs. The
configuration is read through the repo tier (`config --list`), so its
sources are checked as for any repo tier call.

Considered and not taken: writing the objects with the host's git-lfs
(`git hash-object -w --path=<path> <cupper's object>` in the repo tier). It
would follow git-lfs's rules by itself, but runs three or four processes
per object, has git read the whole file into memory, and runs the
programs of `lfs.extension.*` (below).

## IT-git-lfs-stage-objects before the fix

With the copy skipped, each part fails on its own:

1. After staging: `left: None, right: Some(("two\n", "100660", "40770"))`.
2. With that assertion out: the user's `git lfs fsck --objects` exits
   non-zero (the object of the committed pointer is missing).
3. With both out, the turn's revert is refused as
   `conflict stale_diff` ("the patch does not apply"); git said:

   ```
   Downloading a.bin (6 B)
   Error downloading object: a.bin (46210dd): Smudge error: Error downloading a.bin (46210dd…f6b5): batch request: missing protocol: ""
   error: external filter '<git-lfs> filter-process' failed
   fatal: a.bin: smudge filter lfs failed
   ```

   git-lfs went for the LFS server; the test repository has no remote.

## For T237

- git-lfs runs `lfs.extension.<name>.clean` and `.smudge` from the git
  configuration (seen: a marker file made on `git add` with the program in
  `.git/config`); from `.lfsconfig` it does not run them. The repo tier's
  `names_program` does not cover `lfs.extension.*`, so the host's git-lfs
  runs them in the repo tier: a revert's smudge, and any call that cleans,
  such as `git/status`'s `update-index --refresh` (by reading the code, not
  run). Emptying one with `-c lfs.extension.x.clean=` makes git-lfs fail
  instead of skipping it.
- A smudge whose object is missing locally goes to the LFS server. T235
  puts the objects of cupper's own pointers in the user's store; any other
  missing object still sends the host's git-lfs to the network.
- A smudge failure is answered as `stale_diff` (`GitError::Failed` on
  apply), which hides the cause.
