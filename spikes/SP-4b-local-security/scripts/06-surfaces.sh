#!/bin/sh
# Item 6 (minimal, observation only): what another same-user process can see of ours, and which
# signing flags/entitlements govern DYLD_* and task port access. No injection is attempted.
# Usage: SP4B_ID_A=<sha1> sh scripts/06-surfaces.sh
. "$(dirname "$0")/common.sh"
need_identity
build

W=$SP4B_WORK/surfaces
OUT=$LOGS/06
rm -rf "$W" "$OUT"; mkdir -p "$W" "$OUT"
sign_copy "$BIN/peer-client" "$W/target-hardened" "$SP4B_ID_A" "$UI_ID" -o runtime
cp -f "$BIN/peer-client" "$W/target-plain"

{
  echo "## 1. argv / environment of another process owned by the same user"
  for t in target-plain target-hardened; do
    SP4B_CANARY=env-canary-$t "$W/$t" --idle-ms 6000 --name "argv-canary-$t" &
    P=$!
    sleep 0.3
    echo "-- $t"
    echo "ps args:          $(ps -o args= -p "$P" | sed "s#$W#<w>#")"
    echo "ps -E (env) shows canary: $(ps -E -o command= -p "$P" | grep -c "env-canary-$t" || true)"
    echo "ps eww shows canary:      $(ps eww -p "$P" | grep -c "env-canary-$t" || true)"
    echo "from codex read-only sandbox, ps args: $(codex sandbox -c 'sandbox_mode="read-only"' -- /bin/ps -o args= -p "$P" 2>&1 | grep -v '^$' | sed "s#$W#<w>#" | tail -1)"
    echo "from codex read-only sandbox, env canary: $(codex sandbox -c 'sandbox_mode="read-only"' -- /bin/ps -E -o command= -p "$P" 2>/dev/null | grep -c "env-canary-$t" || true)"
    kill "$P" 2>/dev/null || true; wait "$P" 2>/dev/null || true
  done

  echo
  echo "## 2. flags and entitlements that decide DYLD_* and task port access"
  for t in target-plain target-hardened; do
    echo "-- $t"
    codesign -dv "$W/$t" 2>&1 | grep -E '^CodeDirectory' | sed 's/size=[0-9]* //; s/hashes=[^ ]* //'
    ents=$(codesign -d --entitlements - --xml "$W/$t" 2>/dev/null | grep -o 'com\.apple\.security\.[a-z.-]*' | tr '\n' ' ')
    echo "entitlements: ${ents:-none}"
  done
  echo "-- third-party runtimes on this machine (for ADR-08: what a bundled runtime would inherit)"
  for b in "$(command -v node)" /usr/bin/python3 /usr/bin/nc; do
    [ -n "$b" ] || continue
    printf '%s: ' "$(basename "$b")"
    codesign -dv "$b" 2>&1 | grep -E '^CodeDirectory' | sed 's/.*flags=\([^ ]*\).*/flags=\1/' | tr '\n' ' '
    codesign -d --entitlements - --xml "$b" 2>/dev/null | grep -o 'com\.apple\.security\.[a-z.-]*' | tr '\n' ' '
    echo
  done
  echo "-- developer tools security (task_for_pid for debugger-entitled callers needs admin auth when disabled)"
  DevToolsSecurity -status 2>&1
} > "$OUT/surfaces.txt" 2>&1

sanitize < "$OUT/surfaces.txt" > "$RESULTS/06-surfaces.txt"
cat "$RESULTS/06-surfaces.txt"
