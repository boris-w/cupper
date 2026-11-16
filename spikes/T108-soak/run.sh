#!/usr/bin/env bash
# SP-6 scenario 5, the soak, on a runner (T108), as `spike-run` calls it:
# builds the e2e app as T103's run.sh does, then soaks. SP6_SKIP_BUILD=1 uses
# the build there is (a local smoke run).
#
# Without SOAK_MINUTES, the soak takes what the job has left once the build is
# done: SOAK_BUDGET_MINUTES (330 of the job's 360, for the steps before this
# one and the upload after it) less the build's minutes, between 270 and 300.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
started=$(date +%s)
if [ "${SP6_SKIP_BUILD:-}" != 1 ]; then
  (cd "$repo" && cargo xtask runtime-pack && cargo build --locked -p cupper-fake-agent &&
    pnpm --dir apps/desktop build:e2e)
fi
built=$((($(date +%s) - started) / 60))
left=$((${SOAK_BUDGET_MINUTES:-330} - built))
export SOAK_MINUTES=${SOAK_MINUTES:-$((left > 300 ? 300 : left < 270 ? 270 : left))}
mkdir -p "$here/results"
export SP6_RESULTS="$here/results"
{ sysctl -n machdep.cpu.brand_string hw.ncpu hw.memsize; sw_vers; } > "$here/results/machine.txt"
echo "build: $built min; soak: $SOAK_MINUTES min" | tee -a "$here/results/machine.txt"
pnpm --dir "$repo/tools/e2e" exec playwright test -c "$here/playwright.config.ts" \
  --output "$here/results/test-results" "$here/soak.spec.ts"
