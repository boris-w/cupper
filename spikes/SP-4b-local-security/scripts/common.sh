# Shared setup for the SP-4b scripts. Sourced, not executed. POSIX sh.
#
# Environment:
#   SP4B_WORK   scratch directory for builds, signed copies, sockets and logs.
#               Created with mktemp under $TMPDIR when unset. Keep it short:
#               socket paths must fit in sun_path (104 bytes).
#   SP4B_ID_A   SHA-1 of an existing code-signing identity ("our" team).
#   SP4B_ID_B   SHA-1 of a second identity from a different team (optional).
# Identities are only read, never created or imported.

set -eu

SPIKE_DIR=$(cd "$(dirname "$0")/.." && pwd)
SCRIPTS=$SPIKE_DIR/scripts
RESULTS=$SPIKE_DIR/results

if [ -z "${SP4B_WORK:-}" ]; then
  SP4B_WORK=$(mktemp -d "${TMPDIR:-/tmp}/sp4b.XXXXXX")
  echo "SP4B_WORK=$SP4B_WORK" >&2
fi
export SP4B_WORK
BIN=$SP4B_WORK/bin
LOGS=$SP4B_WORK/logs
mkdir -p "$BIN" "$LOGS" "$RESULTS"

UI_ID=dev.cupper.spike.ui
HOST_ID=dev.cupper.spike.host
CLI_ID=dev.cupper.spike.cli

need_identity() {
  if [ -z "${SP4B_ID_A:-}" ]; then
    echo "SP4B_ID_A must be the SHA-1 of an existing code-signing identity" >&2
    echo "(see: security find-identity -v -p codesigning)" >&2
    exit 1
  fi
}

build() {
  CARGO_TARGET_DIR="$SP4B_WORK/target" cargo build --release --quiet \
    --manifest-path "$SPIKE_DIR/rust/Cargo.toml"
  for b in peer-server peer-client bundle-host lineage-host; do
    if [ -f "$SP4B_WORK/target/release/$b" ]; then
      cp -f "$SP4B_WORK/target/release/$b" "$BIN/$b"
    fi
  done
}

# team_of BINARY -> Team ID recorded in its signature (empty for ad-hoc)
team_of() {
  codesign -dv "$1" 2>&1 | sed -n 's/^TeamIdentifier=//p' | grep -v '^not set$' || true
}

# sign_copy SRC DST IDENTITY IDENTIFIER [codesign options...]
sign_copy() {
  src=$1; dst=$2; ident=$3; id=$4; shift 4
  cp -f "$src" "$dst"
  codesign -f -s "$ident" -i "$id" "$@" "$dst" 2>/dev/null
}

wait_for_socket() {
  i=0
  while [ ! -S "$1" ] && [ $i -lt 200 ]; do sleep 0.05; i=$((i + 1)); done
  [ -S "$1" ]
}

# Redact local identifiers before anything is written to results/.
sanitize() {
  python3 -I "$SCRIPTS/sanitize.py"
}

ent_gta() {
  f=$SP4B_WORK/gta.entitlements
  cat > "$f" <<'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>com.apple.security.get-task-allow</key><true/>
</dict></plist>
EOF
  echo "$f"
}
