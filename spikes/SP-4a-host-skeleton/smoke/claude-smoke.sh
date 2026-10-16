#!/bin/bash
# Real-adapter smoke test (SP-4a): new session -> one turn -> one approval ->
# kill -9 the host -> restart -> resume and ask about the previous turn.
#
# Usage: smoke/claude-smoke.sh <adapter-dir>
source "$(dirname "$0")/lib.sh"

log "work dir: $WORK"
start_host
T0=$(ms)
SID=$("${CLI[@]}" create --agent claude --cwd "$REPO" | json '["session_id"]')
log "session $SID created in $(( $(ms) - T0 )) ms"

T0=$(ms)
"${CLI[@]}" send "$SID" "Create a file named hello.txt in the current directory containing exactly the word hi. Do nothing else." --wait \
  > "$WORK/turn1.json" 2> "$WORK/turn1.err" &
SEND=$!
PENDING=$(wait_pending "$SID" "$SEND")
if [ -n "$PENDING" ]; then
  log "permission request after $(( $(ms) - T0 )) ms: $(echo "$PENDING" | json '["request"]["title"]')"
  echo "$PENDING" | json '["request"]["options"]' | sed 's/^/  options: /'
  OPTION=$(echo "$PENDING" | python3 -c '
import json, sys
p = json.load(sys.stdin)
ok = "hello.txt" in json.dumps(p["request"])
kinds = {"allow_once"} if ok else {"reject_once"}
print(next(o["option_id"] for o in p["request"]["options"] if o["kind"] in kinds))')
  log "answering with option $OPTION"
  "${CLI[@]}" approve "$SID" --option "$OPTION"
else
  log "no permission request was raised"
fi
wait "$SEND"
log "turn 1: $(turn_summary "$WORK/turn1.json")"
log "hello.txt: $(cat "$REPO/hello.txt" 2>/dev/null || echo '<missing>')"

PGID=$(sqlite3 "$DATA/host.db" "select pid from adapters")
log "adapter process group $PGID (agent session $(sqlite3 "$DATA/host.db" "select agent_session_id from sessions")):"
group "$PGID" | sed 's/^/  /'
log "host RSS: $(ps -o rss= -p "$HOST") KiB"

log "kill -9 host $HOST"
kill -9 "$HOST"; wait "$HOST" 2>/dev/null || true
sleep 1
log "left in group $PGID 1 s after kill -9:"
group "$PGID" | sed 's/^/  /'

T0=$(ms)
start_host
log "host restarted in $(( $(ms) - T0 )) ms; still alive in group $PGID:"
group "$PGID" | sed 's/^/  /'
"${CLI[@]}" status | sed 's/^/  index: /'

"${CLI[@]}" send "$SID" "What is the name of the file you created earlier in this conversation? Answer with the file name only." --wait \
  > "$WORK/turn2.json" 2> "$WORK/turn2.err"
log "turn 2 (after resume): $(turn_summary "$WORK/turn2.json")"
grep -c 'resuming agent session' "$WORK/host.log" | sed 's/^/  resume log lines: /'

kill -TERM "$HOST"; wait "$HOST" 2>/dev/null || true
log "after graceful stop, adapter records left: $(sqlite3 "$DATA/host.db" "select count(*) from adapters")"
log "done; work dir kept for inspection: $WORK"
