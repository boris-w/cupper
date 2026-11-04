# T234 notes

## Where the change went missing

`git/diff` with `scope: unstaged` runs, on a copy of the user's index (its
mtime kept, design 5.2.10), `write-tree` and then the snapshot (`add -A`,
`write-tree`). The snapshot tier ran git in cupper's own `<data>/git/`.

1. `write-tree` reads the copy and writes it back (it refreshes the cache
   tree). Writing an index, git smudges each racily clean entry: an entry
   whose mtime is not older than the index file's, whose file now has other
   content, gets size 0 so that later reads look at the content
   (`ce_smudge_racily_clean_entry`). It finds the file by `lstat(ce->name)`,
   a path relative to the current directory. `write-tree` does not need a
   work tree and does not change into it, so from `<data>/git/` the `lstat`
   fails and the entry stays as it was.
2. The rewritten copy has a new timestamp, later than the entry's mtime: the
   entry is no longer racy.
3. `add -A` sees an entry whose stat matches the file (same size, same mtime,
   ctime not compared or in the same second) and keeps the old blob. The
   unstaged diff is empty, and `git/stage` of what the user's `git diff`
   shows finds nothing to apply (`stale_diff`).

The user's own index is never written by these calls; their `git diff` sees
the change because their index file still has the racy timestamp.

Evidence, from the investigation: the copy captured from a host process
after `write-tree` and after `add -A` still had `a.txt` at size 2 with the
old blob; giving the copy its old mtime back after `write-tree` made the host
see the change; and the plain git script below.

## Plain git reproduction

Apple Git 2.50.1 and Homebrew git 2.54.0 give the same result:

```sh
set -e
d=$(mktemp -d); export HOME="$d" GIT_CONFIG_NOSYSTEM=1
git init -q -b main "$d/w"; cd "$d/w"
git config core.trustctime false
printf 'a\n' > a.txt; touch -t 202601010000.00 a.txt
git add a.txt; git -c user.name=t -c user.email=t@example.invalid commit -qm init
printf 'b\n' > a.txt; touch -t 202601010000.00 a.txt .git/index
for cwd in "$d/w" "$d"; do
  cp -p .git/index "$d/copy"
  (cd "$cwd" && GIT_DIR="$d/w/.git" GIT_WORK_TREE="$d/w" GIT_INDEX_FILE="$d/copy" git write-tree >/dev/null)
  size=$(GIT_INDEX_FILE="$d/copy" git ls-files --debug a.txt | sed -n 's/.*size: \([0-9]*\).*/\1/p')
  GIT_INDEX_FILE="$d/copy" git add -A
  seen=$(GIT_INDEX_FILE="$d/copy" git diff --cached --name-only HEAD)
  echo "write-tree in ${cwd#$d}/: size $size, add -A then diff --cached: [${seen}]"
done
echo "the user's git diff: [$(git diff --name-only)]"
```

```
write-tree in /w/: size 0, add -A then diff --cached: [a.txt]
write-tree in /: size 2, add -A then diff --cached: []
the user's git diff: [a.txt]
```

## Why the in-process fixtures did not reproduce it

They had no stable "same second": the file's ctime changed with the write,
in another second often enough, and git saw the change by its ctime without
the racy check. T229's process-level case wrote right after the commit, in
the same second, every time.

## The fix

The snapshot tier runs git in the work tree's top (`place.work_tree`, the
directory that holds `.git`, a linked worktree's own top). The work tree is
already git's through `GIT_WORK_TREE`, so the pathspecs do not change; a
subdirectory would add a prefix to them. `git/status` was not affected: its
`update-index --refresh` sets up the work tree itself.

A work tree that is gone before the call is `NotARepo`, as before (`open`
canonicalizes it). One that goes between `open` and the call fails the
spawn's `chdir`: `RunError::Spawn`, then `ShotError::Run`, which the RPCs
answer with `git_error`, a turn snapshot records as `failed`/`git_failed`
and `files/search` answers by walking the directory.

## IT-git-racy-unstaged

The case sets the second instead of waiting for one: the file gets the mtime
the index has for it and the index file gets the same second, a fixed one in
the past. `core.trustctime` is off in the test repository (the snapshot tier
takes it from the repository's configuration): writing the file and setting
its mtime change its ctime. With ctime trusted, on the host before the fix,
the after-commit group passed (the turn took more than a second, so the
ctime was in another second than the one the index recorded) and the
after-staging group failed, but only because its writes fell in the second
of the ctime that `git add` recorded: the outcome hung on timing.

Before the fix:

```
assertion `left == right` failed: after the commit
  left: []
 right: [[String("a.txt"), Number(1), Number(1)]]
```

and, with the first assertion taken out:

```
assertion `left == right` failed: after staging
  left: []
 right: [[String("a.txt"), Number(1), Number(1)]]
```
