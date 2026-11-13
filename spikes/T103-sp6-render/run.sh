#!/usr/bin/env bash
# SP-6 on a runner (T103), as `spike-run` calls it: builds the e2e app as
# e2e-repeat does, then runs the scenarios named, e.g. `bash run.sh 1 2 7`.
# SP6_SKIP_BUILD=1 uses the build there is (a local smoke run). macOS's own
# bash is 3.2: no associative arrays.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
spec() {
  case "$1" in
    1) echo startup ;; 2) echo flood ;; 3) echo four ;; 4) echo long ;;
    7) echo switch ;; 8) echo covered ;;
    *) echo "no scenario $1 (1 2 3 4 7 8)" >&2; return 2 ;;
  esac
}
for n in "$@"; do spec "$n" > /dev/null; done
if [ "${SP6_SKIP_BUILD:-}" != 1 ]; then
  (cd "$repo" && cargo xtask runtime-pack && cargo build --locked -p cupper-fake-agent &&
    pnpm --dir apps/desktop build:e2e)
fi
mkdir -p "$here/results"
export SP6_RESULTS="$here/results"
{ sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize; sw_vers; } > "$here/results/machine.txt"
status=0
for n in "$@"; do
  pnpm --dir "$repo/tools/e2e" exec playwright test -c "$here/playwright.config.ts" \
    "$here/$(spec "$n").spec.ts" || status=1
done
exit "$status"
