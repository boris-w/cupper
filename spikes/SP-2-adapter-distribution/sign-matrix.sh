#!/bin/bash
# Re-sign a JS runtime binary with the hardened runtime and different entitlement
# sets, then check what still works. Ad-hoc signature ("-") by default; pass a
# signing identity hash as $SIGN_ID to use a real certificate instead.
#
# Usage: sign-matrix.sh <runtime binary> <work dir> [<adapter bundle .mjs>]
#   <runtime binary>  e.g. official node, a SEA binary, or a bun --compile output
#   <adapter bundle>  optional; when given (node only), `--version` of it is run
#
# Output: one line per variant and check: OK / FAIL(<exit code or signal>).

set -u
SRC=$1
WORK=$2
BUNDLE=${3:-}
SIGN_ID=${SIGN_ID:--}
KIND=${KIND:-node}   # node: run `-e` checks; exe: the binary is an app, run it with --version
mkdir -p "$WORK"

ent() { # $1 = file, rest = entitlement keys set to true
  local f=$1; shift
  {
    echo '<?xml version="1.0" encoding="UTF-8"?>'
    echo '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">'
    echo '<plist version="1.0"><dict>'
    for k in "$@"; do echo "<key>com.apple.security.cs.$k</key><true/>"; done
    echo '</dict></plist>'
  } > "$f"
}

ent "$WORK/E0.plist"
ent "$WORK/E1.plist" allow-jit
ent "$WORK/E2.plist" allow-jit allow-unsigned-executable-memory
ent "$WORK/E3.plist" allow-jit allow-unsigned-executable-memory disable-library-validation

run() { # $1 label, rest = command; prints OK or FAIL(code)
  local label=$1; shift
  local out
  out=$("$@" 2>&1); local rc=$?
  if [ $rc -eq 0 ]; then echo "  $label: OK"; else echo "  $label: FAIL(rc=$rc) $(echo "$out" | tail -1 | cut -c1-120)"; fi
}

for v in E0 E1 E2 E3; do
  B="$WORK/$(basename "$SRC")-$v"
  cp "$SRC" "$B"
  codesign --remove-signature "$B" 2>/dev/null
  codesign --force --sign "$SIGN_ID" --options runtime --entitlements "$WORK/$v.plist" "$B" 2>&1 | grep -v "replacing existing signature"
  echo "$v ($(sed -n 's/.*com.apple.security.cs.\([a-z-]*\).*/\1/p' "$WORK/$v.plist" | tr '\n' ' '))"
  run "codesign --verify --strict" codesign --verify --strict "$B"
  if [ "$KIND" = node ]; then
    run "node -e (JIT)" "$B" -e 'let s=0;for(let i=0;i<5e7;i++)s+=i%7;'
    run "node --jitless -e" "$B" --jitless -e 'let s=0;for(let i=0;i<1e6;i++)s+=i%7;'
    run "WebAssembly" "$B" -e 'new WebAssembly.Instance(new WebAssembly.Module(new Uint8Array([0,97,115,109,1,0,0,0])))'
    [ -n "$BUNDLE" ] && run "adapter bundle --version" "$B" "$BUNDLE" --version
  else
    run "exe --version" "$B" --version
  fi
done
