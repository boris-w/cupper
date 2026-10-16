#!/bin/bash
# Shared setup for the real-adapter smoke scripts. Source it with the adapter
# directory as $1 (a local install of @agentclientprotocol/claude-agent-acp).
#
# Safety: temp data dir and temp git repo only; permission mode pinned to
# "default"; only "allow once" is ever chosen; cheapest model ("haiku").
set -euo pipefail

SPIKE=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ADAPTER="$1/node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js"
BIN="$SPIKE/target/debug"
NODE=$(command -v node)
CLAUDE=$(command -v claude)
WORK=$(mktemp -d)
DATA="$WORK/data"
REPO="$WORK/repo"
json() { python3 -c "import json,sys; print(json.load(sys.stdin)$1)"; }
ms() { python3 -c 'import time; print(int(time.time()*1000))'; }
log() { echo "[$(date +%H:%M:%S)] $*"; }

mkdir -p "$REPO" && mkdir -m 700 "$DATA"
echo "# smoke" > "$REPO/README.md"
git -C "$REPO" init -q
git -C "$REPO" add README.md
git -C "$REPO" -c user.name=smoke -c user.email=smoke@example.invalid commit -qm init
KEY=$(openssl rand -hex 32)
printf '%s' "$KEY" > "$WORK/key" && chmod 600 "$WORK/key"
cat > "$DATA/agents.json" <<EOF
{"agents":{"claude":{"command":"$NODE","args":["$ADAPTER"],
  "env":{"CLAUDE_CODE_EXECUTABLE":"$CLAUDE"},"model":"haiku","mode":"default"}}}
EOF
CLI=("$BIN/cupper-cli" --data-dir "$DATA" --harness-key-file "$WORK/key")

start_host() {
  printf '%s' "$KEY" | "$BIN/cupper-host" --data-dir "$DATA" --harness-key-stdin 2>>"$WORK/host.log" &
  HOST=$!
  until "${CLI[@]}" status >/dev/null 2>&1; do sleep 0.05; done
}

# Live members of process group $1: pid, rss (KiB), command name.
group() {
  ps -A -o pid=,pgid=,rss=,comm= | awk -v g="$1" '$2==g {n=split($4,p,"/"); print $1, $3, p[n]}'
}

# Poll until session $1 has a pending interaction (or $2, a pid, exits); print it.
wait_pending() {
  local p=""
  for _ in $(seq 1 600); do
    p=$("${CLI[@]}" pending "$1" | head -1)
    [ -n "$p" ] && break
    kill -0 "$2" 2>/dev/null || break
    sleep 0.2
  done
  echo "$p"
}

turn_summary() { # last line of a `send --wait` output file
  tail -1 "$1" | python3 -c 'import json,sys; d=json.load(sys.stdin); print(d["status"], d["stop_reason"], str(d["elapsed_ms"]) + " ms", repr(d["agent_text"][:200]))'
}
