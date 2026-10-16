#!/bin/sh
# Item 3: replace the whole .app while the helper host is running (scaled-down ADR-01 drill).
# Usage: SP4B_ID_A=<sha1> sh scripts/03-upgrade.sh
. "$(dirname "$0")/common.sh"
need_identity
build

W=$SP4B_WORK/upgrade
OUT=$LOGS/03
rm -rf "$W" "$OUT"; mkdir -p "$W/stage" "$W/inst" "$OUT"
S=$W/host.sock

plist() { # file id exe version
  cat > "$1" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>$2</string>
<key>CFBundleExecutable</key><string>$3</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$4</string>
<key>CFBundleVersion</key><string>$4</string>
<key>LSUIElement</key><true/>
</dict></plist>
EOF
}

make_app() { # version
  v=$1; app=$W/stage/v$v/Test.app; h=$app/Contents/Helpers/TestHost.app
  mkdir -p "$app/Contents/MacOS" "$h/Contents/MacOS" "$h/Contents/Resources"
  plist "$app/Contents/Info.plist" "$UI_ID" Test "$v.0"
  plist "$h/Contents/Info.plist" "$HOST_ID" TestHost "$v.0"
  echo "$v" > "$h/Contents/Resources/version.txt"
  cp "$BIN/bundle-host" "$h/Contents/MacOS/TestHost"
  cp "$BIN/peer-client" "$h/Contents/MacOS/tool"
  cp "$BIN/peer-client" "$app/Contents/MacOS/Test"
  # inside-out signing; the tool's identifier differs per version so its cdhash differs
  codesign -f -s "$SP4B_ID_A" -o runtime -i "dev.cupper.spike.tool.v$v" "$h/Contents/MacOS/tool" 2>/dev/null
  codesign -f -s "$SP4B_ID_A" -o runtime "$h" 2>/dev/null
  codesign -f -s "$SP4B_ID_A" -o runtime "$app" 2>/dev/null
  codesign --verify --deep --strict "$app"
}
make_app 1
make_app 2
SP4B_TEAM_A=$(team_of "$W/stage/v1/Test.app"); export SP4B_TEAM_A
REQ_HOST="anchor apple generic and identifier \"$HOST_ID\" and certificate leaf[subject.OU] = \"$SP4B_TEAM_A\""

APP=$W/inst/Test.app
HAPP=$APP/Contents/Helpers/TestHost.app
HEXE=$HAPP/Contents/MacOS/TestHost

cmd() { # send one command to the host, print the reply
  /usr/bin/python3 -c 'import socket,sys
s=socket.socket(socket.AF_UNIX); s.settimeout(3)
try:
    s.connect(sys.argv[1]); s.sendall((sys.argv[2]+"\n").encode()); print(s.makefile().readline().strip())
except Exception as e:
    print("{\"cmd\": \"%s\", \"error\": \"%s\"}" % (sys.argv[2], e))' "$S" "$1"
}

ui_check() { # client-binary label [flags]
  out=$("$1" --socket "$S" --name "ui-check-$2" --server-require "$REQ_HOST" --server-flags "${3:-default}" 2>&1 | head -1)
  python3 -I -c 'import json,sys
try:
    d=json.loads(sys.argv[1])["server_check"]
    e=(d["detail"].get("error") or d["detail"])
    print(json.dumps({"ui_check": sys.argv[2], "flags": sys.argv[3], "accept": d["accept"], "stage": d["stage"], "status": e.get("status"), "message": e.get("message")}, ensure_ascii=False))
except Exception as x:
    print(json.dumps({"ui_check": sys.argv[2], "raw": sys.argv[1][:300]}))' "$out" "$2" "${3:-default}"
}

probe_all() { # label client-binary
  echo "-- $1"
  echo "alive=$(kill -0 "$HP" 2>/dev/null && echo yes || echo no) inode(TestHost on disk)=$(stat -f %i "$HEXE" 2>/dev/null || echo missing)"
  for c in ping selfcheck spawn readres; do cmd "$c"; done
  ui_check "$2" "$1" default
  ui_check "$2" "$1" kernel
}

start_v1() {
  rm -rf "$W/inst/"* "$W/old"; mkdir -p "$W/old"
  rm -f "$S"
  ditto "$W/stage/v1/Test.app" "$APP"
  "$HEXE" --socket "$S" --bundle "$HAPP" --runtime "$HAPP/Contents/MacOS/tool" 2>>"$OUT/host.err" &
  HP=$!
  wait_for_socket "$S"
}

scenario() { # name, replacement command (string)
  echo "## $1"
  start_v1
  probe_all "before" "$APP/Contents/MacOS/Test"
  echo "replace: $2"
  sh -c "$2"
  sleep 0.5
  probe_all "after" "$APP/Contents/MacOS/Test"
  kill "$HP" 2>/dev/null || true; wait "$HP" 2>/dev/null || true
}

{
  scenario "S1 Sparkle-style: stage new copy, move old aside, move new in, delete old" \
    "ditto '$W/stage/v2/Test.app' '$W/inst/.Test.app.new' && mv '$APP' '$W/old/Test.app' && mv '$W/inst/.Test.app.new' '$APP' && rm -rf '$W/old/Test.app'"
  scenario "S2 same, old bundle kept at another path (not deleted)" \
    "ditto '$W/stage/v2/Test.app' '$W/inst/.Test.app.new' && mv '$APP' '$W/old/Test.app' && mv '$W/inst/.Test.app.new' '$APP'"
  scenario "S3 ditto v2 over the installed bundle in place" \
    "ditto '$W/stage/v2/Test.app' '$APP'"
  scenario "S4 cp v2 helper executable over the running file in place (what an installer must never do)" \
    "cp '$W/stage/v2/Test.app/Contents/Helpers/TestHost.app/Contents/MacOS/TestHost' '$HEXE'"

  echo "## S5 switch: old host gone, new UI launches the new host from the new bundle"
  start_v1
  ditto "$W/stage/v2/Test.app" "$W/inst/.Test.app.new"; mv "$APP" "$W/old/Test.app"; mv "$W/inst/.Test.app.new" "$APP"; rm -rf "$W/old/Test.app"
  cmd selfcheck
  kill "$HP"; wait "$HP" 2>/dev/null || true
  rm -f "$S"
  "$HEXE" --socket "$S" --bundle "$HAPP" --runtime "$HAPP/Contents/MacOS/tool" 2>>"$OUT/host.err" &
  HP=$!; wait_for_socket "$S"
  probe_all "new host" "$APP/Contents/MacOS/Test"
  kill "$HP" 2>/dev/null || true; wait "$HP" 2>/dev/null || true
} > "$OUT/upgrade.txt" 2>&1

sanitize < "$OUT/upgrade.txt" > "$RESULTS/03-upgrade.txt"
cat "$RESULTS/03-upgrade.txt"
