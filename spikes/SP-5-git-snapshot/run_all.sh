#!/usr/bin/env bash
# Run the whole SP-5 spike end to end. Safe to re-run: sample repos are reused,
# scratch repos are recreated. Everything lives under $SP5_WORKDIR.
#
#   SP5_WORKDIR=/some/tmp/dir ./run_all.sh            # full run (~25 min, ~8 GB disk)
#   SKIP_LINUX=1 ./run_all.sh                          # synthetic repos only
#   SKIP_LIBGIT2=1 ./run_all.sh                        # no pygit2/uv needed
#   SKIP_GIX=1 ./run_all.sh                            # no cargo build of the gitoxide probe
#
# Optional: put a git-lfs binary in $SP5_WORKDIR/tools/bin to run the LFS cases
# (it is only used through repo-local config; nothing global is installed).
set -euo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
export SP5_WORKDIR="${SP5_WORKDIR:-${TMPDIR:-/tmp}/cupper-sp5}"
mkdir -p "$SP5_WORKDIR"
cd "$SP5_WORKDIR"   # never run from inside a sample repo

REPOS="syn-1k,syn-10k,syn-100k,linux"
SETUP_ARGS=()
if [[ "${SKIP_LINUX:-}" == 1 ]]; then REPOS="syn-1k,syn-10k,syn-100k"; SETUP_ARGS+=(--skip-linux); fi

# python -I (isolated: no PYTHON* env, cwd not on sys.path) drops the script dir from
# sys.path too, so a tiny wrapper adds it back explicitly.
run() {
  echo "== $*"
  python3 -I -c 'import runpy,sys; sys.path.insert(0, sys.argv[1]); sys.argv = sys.argv[2:]; runpy.run_path(sys.argv[0], run_name="__main__")' \
    "$HERE" "$HERE/$1" "${@:2}"
}

run setup_repos.py ${SETUP_ARGS[@]+"${SETUP_ARGS[@]}"}   # bash 3.2-safe empty array
run edge_hunk.py
run edge_snapshot.py
run edge_worktree.py
run bench_snapshot.py --repos "$REPOS"
run bench_worktree.py --repos "$REPOS"
if [[ "${SKIP_LINUX:-}" == 1 ]]; then run bench_shadow.py --repos syn-10k; else run bench_shadow.py; fi
if [[ "${SKIP_LIBGIT2:-}" != 1 ]]; then
  export UV_CACHE_DIR="$SP5_WORKDIR/uv-cache"
  for s in "edge_hunk.py --backend libgit2" "edge_hunk.py --backend libgit2+fix" "compare_libgit2.py --repos $REPOS"; do
    echo "== (libgit2) $s"
    # shellcheck disable=SC2086
    uv run --quiet --no-project --with pygit2 python -I -c 'import runpy,sys; sys.path.insert(0, sys.argv[1]); sys.argv = sys.argv[2:]; runpy.run_path(sys.argv[0], run_name="__main__")' \
      "$HERE" "$HERE/"$s
  done
fi
if [[ "${SKIP_GIX:-}" != 1 ]] && command -v cargo >/dev/null; then
  run compare_gix.py --repos "$REPOS"
fi
run render.py
echo "results: $SP5_WORKDIR/results"
