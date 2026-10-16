#!/bin/bash
# Real-adapter crash test (SP-4a): kill -9 the host while the agent waits on a
# permission request, restart, check recovery (turn interrupted, request
# expired, no leftover processes), then resume and continue the conversation.
#
# Usage: smoke/claude-midturn.sh <adapter-dir>
source "$(dirname "$0")/lib.sh"

log "work dir: $WORK"
start_host
SID=$("${CLI[@]}" create --agent claude --cwd "$REPO" | json '["session_id"]')
log "session $SID created"

"${CLI[@]}" send "$SID" "Create a file named hello.txt in the current directory containing exactly the word hi. Do nothing else." --wait \
  > "$WORK/turn1.json" 2> "$WORK/turn1.err" &
SEND=$!
PENDING=$(wait_pending "$SID" "$SEND")
[ -n "$PENDING" ] || { log "no permission request was raised"; exit 1; }
log "pending (left unanswered): $(echo "$PENDING" | json '["request"]["title"]')"

PGID=$(sqlite3 "$DATA/host.db" "select pid from adapters")
log "adapter process group $PGID while waiting for approval:"
group "$PGID" | sed 's/^/  /'

log "kill -9 host $HOST"
kill -9 "$HOST"; wait "$HOST" 2>/dev/null || true
kill "$SEND" 2>/dev/null || true
sleep 1
log "left in group $PGID 1 s after kill -9:"
group "$PGID" | sed 's/^/  /'

T0=$(ms)
start_host
log "host restarted in $(( $(ms) - T0 )) ms; still alive in group $PGID:"
group "$PGID" | sed 's/^/  /'
grep -E 'reaping|recovery done' "$WORK/host.log" | sed -E 's/\x1b\[[0-9;]*m//g; s/^.*(INFO|WARN)/  \1/'
"${CLI[@]}" status | sed 's/^/  index: /'
"${CLI[@]}" watch "session/$SID" --idle-ms 300 | python3 -c '
import json, sys
for line in sys.stdin:
    m = json.loads(line)
    for op in m.get("ops", []):
        if op["type"] in ("turn.completed", "interaction.resolved"):
            print("  snapshot:", op["type"], json.dumps(op["data"]))'
log "hello.txt: $(cat "$REPO/hello.txt" 2>/dev/null || echo '<missing>')"

"${CLI[@]}" send "$SID" "Did the file hello.txt get created? Look at the current directory and answer yes or no only." --wait \
  > "$WORK/turn2.json" 2> "$WORK/turn2.err" &
SEND=$!
PENDING=$(wait_pending "$SID" "$SEND")
if [ -n "$PENDING" ]; then
  # A read-only check (ls) may need approval in default mode; allow once, reject anything else.
  OPTION=$(echo "$PENDING" | python3 -c '
import json, sys
p = json.load(sys.stdin)
title = p["request"]["title"]
ok = title.strip().startswith("ls")
kinds = {"allow_once"} if ok else {"reject_once"}
print(next(o["option_id"] for o in p["request"]["options"] if o["kind"] in kinds))')
  log "request '$(echo "$PENDING" | json '["request"]["title"]')' -> $OPTION"
  "${CLI[@]}" approve "$SID" --option "$OPTION"
fi
wait "$SEND"
log "turn 2 (after resume): $(turn_summary "$WORK/turn2.json")"

kill -TERM "$HOST"; wait "$HOST" 2>/dev/null || true
log "after graceful stop, adapter records left: $(sqlite3 "$DATA/host.db" "select count(*) from adapters")"
log "done; work dir kept for inspection: $WORK"
