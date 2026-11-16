#!/usr/bin/env bash
# SP-6 on a runner (T103), as `spike-run` calls it: builds the e2e app as
# e2e-repeat does, then runs the scenarios named, e.g. `bash run.sh 1 2 7`.
# SP6_SKIP_BUILD=1 uses the build there is (a local smoke run). macOS's own
# bash is 3.2: no associative arrays.
# T159: SP6_TRACE=1 records Chromium traces (trace.ts) and sums them up after
# the cases (summarize.mjs); SP6_UNMINIFIED=1 builds the renderer unminified,
# so that a sampled function has its name and its module.
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
  (cd "$repo" && cargo xtask runtime-pack && cargo build --locked -p cupper-fake-agent)
  if [ "${SP6_UNMINIFIED:-}" = 1 ]; then
    # build:e2e as it is, with the renderer's `vite build` unminified.
    script=$(cd "$repo/apps/desktop" && node -p 'require("./package.json").scripts["build:e2e"]')
    case "$script" in
      *"vite build --mode e2e "*) ;;
      *) echo "build:e2e has no 'vite build --mode e2e': $script" >&2; exit 1 ;;
    esac
    pnpm --dir "$repo/apps/desktop" exec bash -c "${script/vite build --mode e2e /vite build --mode e2e --minify false }"
  else
    pnpm --dir "$repo/apps/desktop" build:e2e
  fi
fi
mkdir -p "$here/results"
export SP6_RESULTS="$here/results"
{ sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize; sw_vers; } > "$here/results/machine.txt"
status=0
for n in "$@"; do
  # A folder of its own: a Playwright run empties its output folder first.
  pnpm --dir "$repo/tools/e2e" exec playwright test -c "$here/playwright.config.ts" \
    --output "$here/results/test-results-$n" "$here/$(spec "$n").spec.ts" || status=1
done
if [ "${SP6_TRACE:-}" = 1 ]; then
  node "$here/summarize.mjs" "$here/results" "$repo/apps/desktop/dist/renderer/assets" || status=1
fi
exit "$status"
