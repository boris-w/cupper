#!/bin/sh
# Items 1 + 2: host verifies clients by LOCAL_PEERTOKEN -> SecCode -> requirement + flags,
# and the reverse direction (client verifies host, including a hijacked socket path).
# Usage: SP4B_ID_A=<sha1> [SP4B_ID_B=<sha1>] sh scripts/01-peer-verify.sh
. "$(dirname "$0")/common.sh"
need_identity
build

C=$SP4B_WORK/clients
RUN=$SP4B_WORK/run01
rm -rf "$C" "$RUN"; mkdir -p "$C" "$RUN"
chmod 700 "$RUN"
S=$RUN/host.sock
OUT=$LOGS/01
rm -rf "$OUT"; mkdir -p "$OUT"
PC=$BIN/peer-client
GTA=$(ent_gta)

# ---- client variants -------------------------------------------------------
sign_copy "$PC" "$C/ui-good"      "$SP4B_ID_A" "$UI_ID" -o runtime
sign_copy "$PC" "$C/ui-other-id"  "$SP4B_ID_A" "$CLI_ID" -o runtime
sign_copy "$PC" "$C/ui-helper-id" "$SP4B_ID_A" "$UI_ID.helper" -o runtime
sign_copy "$PC" "$C/ui-noruntime" "$SP4B_ID_A" "$UI_ID"
sign_copy "$PC" "$C/ui-gta"       "$SP4B_ID_A" "$UI_ID" -o runtime --entitlements "$GTA"
sign_copy "$PC" "$C/ui-adhoc"     -            "$UI_ID" -o runtime
cp -f "$PC" "$C/ui-linker"                          # linker-signed (cargo default on arm64)
cp -f "$PC" "$C/ui-unsigned"; codesign --remove-signature "$C/ui-unsigned"
if [ -n "${SP4B_ID_B:-}" ]; then
  sign_copy "$PC" "$C/ui-other-team" "$SP4B_ID_B" "$UI_ID" -o runtime
fi
SP4B_TEAM_A=$(team_of "$C/ui-good"); export SP4B_TEAM_A
if [ -n "${SP4B_ID_B:-}" ]; then SP4B_TEAM_B=$(team_of "$C/ui-other-team"); export SP4B_TEAM_B; fi

{
  for f in "$C"/*; do
    echo "== $(basename "$f")"
    codesign -dv "$f" 2>&1 | grep -E '^(Identifier|CodeDirectory|TeamIdentifier|Signature)=?' || echo "(no signature)"
  done
} > "$OUT/clients.txt" 2>&1

REQ_TEAM="anchor apple generic and identifier \"$UI_ID\" and certificate leaf[subject.OU] = \"$SP4B_TEAM_A\""
REQ_DEVID="$REQ_TEAM and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists"
REQ_IDONLY="identifier \"$UI_ID\""
CDHASH_ADHOC=$(codesign -dvvv "$C/ui-adhoc" 2>&1 | sed -n 's/^CDHash=//p')
REQ_CDHASH="cdhash H\"$CDHASH_ADHOC\""

start_server() { # name, server args...
  name=$1; shift
  rm -f "$S"
  "$BIN/peer-server" --socket "$S" --log "$OUT/$name.server.jsonl" "$@" >/dev/null 2>>"$OUT/$name.server.err" &
  SERVER_PID=$!
  wait_for_socket "$S" || echo "server $name did not start" >&2
}
stop_server() { kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }

run_clients() { # name
  name=$1
  out=$OUT/$name.clients.txt
  : > "$out"
  for f in ui-good ui-other-id ui-helper-id ui-other-team ui-noruntime ui-gta ui-adhoc ui-linker ui-unsigned; do
    [ -f "$C/$f" ] || continue
    rc=0; "$C/$f" --socket "$S" --name "$f" >>"$out" 2>&1 || rc=$?
    echo "client=$f exit=$rc" >>"$out"
  done
  rc=0; node -e 'const s=require("net").connect(process.argv[1],()=>s.write("hello from node\n"));s.on("data",d=>{process.stdout.write(d);s.end()})' "$S" >>"$out" 2>&1 || rc=$?
  echo "client=node exit=$rc" >>"$out"
  rc=0; /usr/bin/python3 -c 'import socket,sys;s=socket.socket(socket.AF_UNIX);s.connect(sys.argv[1]);s.sendall(b"hello from python3\n");print(s.recv(4096).decode().strip())' "$S" >>"$out" 2>&1 || rc=$?
  echo "client=python3 exit=$rc" >>"$out"
  rc=0; printf 'hello from nc\n' | nc -U -w 2 "$S" >>"$out" 2>&1 || rc=$?
  echo "client=nc exit=$rc" >>"$out"
}

suite() { # name, server args...
  name=$1; shift
  start_server "$name" "$@"
  run_clients "$name"
  stop_server
}

# ---- item 1/2: requirement variants ----------------------------------------
suite A-team          --require "$REQ_TEAM"
suite B-team-kernel   --require "$REQ_TEAM" --flags kernel
suite C-devid         --require "$REQ_DEVID"
suite D-idonly        --require "$REQ_IDONLY"
suite E-cdhash-adhoc  --require "$REQ_CDHASH"

# ---- reverse direction: UI verifies host -----------------------------------
sign_copy "$BIN/peer-server" "$C/host-good"  "$SP4B_ID_A" "$HOST_ID" -o runtime
sign_copy "$BIN/peer-server" "$C/host-fake"  -            "$HOST_ID" -o runtime
REQ_HOST="anchor apple generic and identifier \"$HOST_ID\" and certificate leaf[subject.OU] = \"$SP4B_TEAM_A\""
{
  rm -f "$S"
  "$C/host-good" --socket "$S" --require "$REQ_TEAM" --log "$OUT/F-reverse.server.jsonl" >/dev/null 2>&1 &
  HP=$!; wait_for_socket "$S"
  echo "## genuine host, genuine UI"
  rc=0; "$C/ui-good" --socket "$S" --name ui-good --server-require "$REQ_HOST" || rc=$?; echo "exit=$rc"
  echo "## socket hijack: attacker unlinks the path and binds a fake host (genuine host still running: $(kill -0 $HP && echo yes))"
  ls -ld "$RUN" | awk '{print "dir mode:", $1}'
  rm -f "$S"
  "$C/host-fake" --socket "$S" --no-runtime-check --log "$OUT/F-reverse.fake.jsonl" >/dev/null 2>&1 &
  FP=$!; wait_for_socket "$S"
  rc=0; "$C/ui-good" --socket "$S" --name ui-good --server-require "$REQ_HOST" || rc=$?; echo "exit=$rc"
  echo "## same, without client-side verification (what a naive client would do)"
  rc=0; "$C/ui-good" --socket "$S" --name ui-good-naive || rc=$?; echo "exit=$rc"
  kill $FP $HP 2>/dev/null || true; wait 2>/dev/null || true
} > "$OUT/F-reverse.txt" 2>&1

# ---- edge cases: stale tokens, exec / fork tricks, sandboxed verifier ------
{
  echo "## G1 peer exits before the host checks (server delay 300 ms)"
  start_server G1 --require "$REQ_TEAM" --delay-ms 300
  "$C/ui-good" --socket "$S" --name G1-exited --no-wait || true
  sleep 0.6; stop_server

  echo "## G2 attacker connects, then execs the genuine binary which talks on the inherited fd"
  start_server G2 --require "$REQ_TEAM"
  "$C/ui-adhoc" --socket "$S" --name G2-attacker --exec -- "$C/ui-good" --fd 9 --name G2-genuine-after-exec || true
  stop_server

  echo "## G3 attacker writes first, then execs the genuine binary (idle); host checks 300 ms later"
  start_server G3 --require "$REQ_TEAM" --delay-ms 300
  "$C/ui-adhoc" --socket "$S" --name G3-attacker --write-before-exec --exec -- "$C/ui-good" --idle-ms 1500 || true
  sleep 0.3; stop_server

  echo "## G4 fork+exec: child writes then execs the genuine binary; parent keeps talking (check once)"
  start_server G4 --require "$REQ_TEAM" --delay-ms 300
  "$C/ui-adhoc" --socket "$S" --name G4-attacker --lines 2 --fork-exec -- "$C/ui-good" --idle-ms 2000 || true
  stop_server

  echo "## G5 same as G4 but the host re-checks every message"
  start_server G5 --require "$REQ_TEAM" --delay-ms 300 --recheck
  "$C/ui-adhoc" --socket "$S" --name G5-attacker --lines 2 --fork-exec -- "$C/ui-good" --idle-ms 2000 || true
  stop_server

  echo "## G6 genuine client leaks its authenticated fd to an exec'd child (no CLOEXEC); child talks"
  start_server G6 --require "$REQ_TEAM"
  "$C/ui-good" --socket "$S" --name G6-genuine --write-before-exec --exec -- "$C/ui-adhoc" --fd 9 --name G6-child-on-leaked-fd --lines 2 || true
  stop_server

  echo "## G7 same as G6 with per-message re-check"
  start_server G7 --require "$REQ_TEAM" --recheck
  "$C/ui-good" --socket "$S" --name G7-genuine --write-before-exec --exec -- "$C/ui-adhoc" --fd 9 --name G7-child-on-leaked-fd --lines 2 || true
  stop_server

  echo "## P1 pinned mode (--pin): genuine client answers the challenge"
  start_server P1 --require "$REQ_TEAM" --pin
  "$C/ui-good" --socket "$S" --name P1-genuine --challenge --lines 2 || true
  "$C/ui-adhoc" --socket "$S" --name P1-adhoc --challenge || true
  stop_server

  echo "## P2 pinned mode vs G3 (write, then exec the genuine binary)"
  start_server P2 --require "$REQ_TEAM" --pin --delay-ms 300
  "$C/ui-adhoc" --socket "$S" --name P2-attacker --challenge --write-before-exec --exec -- "$C/ui-good" --idle-ms 1500 || true
  sleep 0.3; stop_server

  echo "## P3 pinned mode vs G4 (fork; child execs the genuine binary; parent talks)"
  start_server P3 --require "$REQ_TEAM" --pin --delay-ms 300
  "$C/ui-adhoc" --socket "$S" --name P3-attacker --lines 2 --fork-exec -- "$C/ui-good" --idle-ms 2000 || true
  stop_server

  echo "## P4 pinned mode vs G6 (genuine client leaks its fd to an exec'd child)"
  start_server P4 --require "$REQ_TEAM" --pin
  "$C/ui-good" --socket "$S" --name P4-genuine --challenge --write-before-exec --exec -- "$C/ui-adhoc" --fd 9 --name P4-child-on-leaked-fd --lines 2 || true
  stop_server

  for prof in '(version 1)(allow default)(deny process-info-codesignature)'; do
    echo "## G8 verifier itself runs in a sandbox: $prof"
    rm -f "$S"
    sandbox-exec -p "$prof" "$BIN/peer-server" --socket "$S" --require "$REQ_TEAM" --log "$OUT/G8.server.jsonl" >/dev/null 2>>"$OUT/G8.server.err" &
    SERVER_PID=$!
    if wait_for_socket "$S"; then
      "$C/ui-good" --socket "$S" --name "G8-$(echo "$prof" | sed 's/.*(deny \(.*\)))/\1/')" || true
    else
      echo "verifier did not start under this profile"
    fi
    stop_server
  done
} > "$OUT/G-edge.txt" 2>&1

# ---- timing: 20 sequential verifications of the genuine client --------------
start_server H-timing --require "$REQ_TEAM"
i=0; while [ $i -lt 20 ]; do "$C/ui-good" --socket "$S" --name "H$i" >/dev/null 2>&1 || true; i=$((i + 1)); done
stop_server

python3 -I "$SCRIPTS/summarize.py" "$OUT" > "$OUT/summary.txt"
cat "$OUT/summary.txt"

for f in "$OUT"/*; do
  sanitize < "$f" > "$RESULTS/01-$(basename "$f")"
done
echo "results written to results/01-*" >&2
