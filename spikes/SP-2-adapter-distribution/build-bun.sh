#!/bin/bash
# Compile an adapter entry into a single executable with `bun build --compile`,
# then re-sign it ad-hoc with the hardened runtime and the given entitlements.
#
# Usage: build-bun.sh <bun binary> <entry .js> <output binary> <entitlements.plist> <scratch dir>
# The scratch dir isolates bun's caches from $HOME ($SIGN_ID overrides "-").

set -euo pipefail
BUN=$1; ENTRY=$2; OUT=$3; ENT=$4; SCRATCH=$5
SIGN_ID=${SIGN_ID:--}
mkdir -p "$SCRATCH" "$(dirname "$OUT")"
export BUN_INSTALL="$SCRATCH/bun-install"
export BUN_INSTALL_CACHE_DIR="$SCRATCH/bun-cache"
export BUN_RUNTIME_TRANSPILER_CACHE_PATH=0
export DO_NOT_TRACK=1

"$BUN" build --compile --target=bun-darwin-arm64 "$ENTRY" --outfile "$OUT"
codesign --remove-signature "$OUT"
codesign --force --sign "$SIGN_ID" --options runtime --entitlements "$ENT" "$OUT"
codesign --verify --strict "$OUT"
ls -la "$OUT" | awk '{print $5, $NF}'
