#!/bin/bash
# Build a Node single executable application (SEA) from one bundled script.
#
# Usage: build-sea.sh <node binary> <main script> <output binary> <postject bin> <entitlements.plist> [main format: commonjs|module]
# The node binary must be the exact version whose blob format is used (same
# binary runs --experimental-sea-config). Output is re-signed ad-hoc with the
# hardened runtime and the given entitlements ($SIGN_ID overrides "-").

set -euo pipefail
NODE=$1; MAIN=$2; OUT=$3; POSTJECT=$4; ENT=$5; FORMAT=${6:-commonjs}
SIGN_ID=${SIGN_ID:--}
WORK=$(dirname "$OUT")
mkdir -p "$WORK"
CFG="$OUT.sea-config.json"
BLOB="$OUT.blob"

if [ "$FORMAT" = module ]; then FMT_LINE='"mainFormat": "module",'; else FMT_LINE=''; fi
cat > "$CFG" <<EOF
{
  "main": "$MAIN",
  "output": "$BLOB",
  $FMT_LINE
  "disableExperimentalSEAWarning": true,
  "useCodeCache": false,
  "useSnapshot": false
}
EOF

"$NODE" --experimental-sea-config "$CFG"
cp "$NODE" "$OUT"
codesign --remove-signature "$OUT"
"$POSTJECT" "$OUT" NODE_SEA_BLOB "$BLOB" \
  --sentinel-fuse NODE_SEA_FUSE_fce680ab2cc467b6e072b8b5df1996b2 \
  --macho-segment-name NODE_SEA >/dev/null
codesign --force --sign "$SIGN_ID" --options runtime --entitlements "$ENT" "$OUT"
codesign --verify --strict "$OUT"
ls -la "$OUT" | awk '{print $5, $NF}'
