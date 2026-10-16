#!/bin/sh
# Item 4: can a command run by an agent connect to the host socket?
#
# Three listening sockets (the server log is the ground truth):
#   tmpdir     <work>/agent/run/host.sock         ($TMPDIR, the ADR-05 fallback location)
#   workspace  <work>/agent/repo/.cupper/host.sock (inside the agent's workspace)
#   cache      $DARWIN_USER_CACHE_DIR/sp4b-agent.*/host.sock (neither TMPDIR nor workspace)
#
# Part B (always): Codex's own seatbelt runner, no model involved.
# Part C (only with SP4B_RUN_AGENTS=1): one real turn each with Claude Code (sandbox on, default
# permissions) and Codex exec (read-only, workspace-write, Agent = workspace-write + on-request).
# No full-access/bypass modes; nothing is ever "always allowed"; ~/.claude and ~/.codex are only
# checksummed before and after.
. "$(dirname "$0")/common.sh"
build

A_DIR=$SP4B_WORK/agent
OUT=$LOGS/04
rm -rf "$A_DIR" "$OUT"; mkdir -p "$A_DIR/run" "$A_DIR/repo/.cupper" "$OUT"
chmod 700 "$A_DIR/run"
CACHE_DIR=$(mktemp -d "$(getconf DARWIN_USER_CACHE_DIR)sp4b-agent.XXXXXX")
SOCK_TMP=$A_DIR/run/host.sock
SOCK_WS=$A_DIR/repo/.cupper/host.sock
SOCK_CACHE=$CACHE_DIR/host.sock
REPO=$A_DIR/repo

cat > "$REPO/probe.sh" <<EOF
#!/bin/sh
# Local connectivity check: try to reach three Unix sockets, one line per attempt.
for spec in "tmpdir:$SOCK_TMP" "workspace:$SOCK_WS" "cache:$SOCK_CACHE"; do
  label=\${spec%%:*}; path=\${spec#*:}
  /usr/bin/python3 -c 'import socket,sys
s=socket.socket(socket.AF_UNIX); s.settimeout(3)
try:
    s.connect(sys.argv[2]); s.sendall(("probe %s python3\\n" % sys.argv[1]).encode()); s.recv(200)
    print(sys.argv[1], "python3 CONNECTED")
except Exception as e:
    print(sys.argv[1], "python3 FAILED:", e)' "\$label" "\$path"
  if printf 'probe %s nc\n' "\$label" | nc -U -w 2 "\$path" >/dev/null 2>&1; then
    echo "\$label nc CONNECTED"
  else
    echo "\$label nc FAILED (exit \$?)"
  fi
done
EOF
printf '# scratch repo for SP-4b\n' > "$REPO/README.md"
git -C "$REPO" init -q
git -C "$REPO" add -A
git -C "$REPO" -c user.name=sp4b -c user.email=sp4b@example.invalid commit -qm init

start_server() { # name
  rm -f "$SOCK_TMP" "$SOCK_WS" "$SOCK_CACHE"
  "$BIN/peer-server" --socket "$SOCK_TMP" --socket "$SOCK_WS" --socket "$SOCK_CACHE" \
    --no-runtime-check --no-gta-check --log "$OUT/$1.server.jsonl" >/dev/null 2>>"$OUT/server.err" &
  SERVER_PID=$!
  wait_for_socket "$SOCK_TMP"; wait_for_socket "$SOCK_WS"; wait_for_socket "$SOCK_CACHE"
  : > "$OUT/$1.server.jsonl"
}
stop_server() { kill "$SERVER_PID" 2>/dev/null || true; wait "$SERVER_PID" 2>/dev/null || true; }

fingerprint() { # snapshot of agent config files (content hash + mtime), never their content
  for f in "$HOME/.claude/settings.json" "$HOME/.claude.json" "$HOME/.codex/config.toml"; do
    if [ -f "$f" ]; then
      echo "$(shasum -a 256 "$f" | cut -c1-16) $(stat -f %m "$f") $f"
    fi
  done
}

# ---- B: Codex seatbelt runner, no model ---------------------------------------
{
  for spec in \
    "unsandboxed|" \
    "codex-read-only|-c sandbox_mode=\"read-only\"" \
    "codex-workspace-write|-c sandbox_mode=\"workspace-write\"" \
    "codex-workspace-write+network|-c sandbox_mode=\"workspace-write\" -c sandbox_workspace_write.network_access=true" \
    "codex-workspace-write+allow-unix-socket(tmpdir)|-c sandbox_mode=\"workspace-write\" --allow-unix-socket $A_DIR/run"; do
    name=${spec%%|*}; args=${spec#*|}
    echo "## $name"
    start_server "B-$name"
    if [ "$name" = unsandboxed ]; then
      (cd "$REPO" && sh probe.sh) 2>&1
    else
      # shellcheck disable=SC2086
      (cd "$REPO" && eval codex sandbox $args -- /bin/sh probe.sh) 2>&1 | grep -v '^$'
    fi
    stop_server
    echo "server saw: $(grep -c '"msg_index":1' "$OUT/B-$name.server.jsonl" || true) connection(s)"
  done
} > "$OUT/B-codex-runner.txt" 2>&1

# ---- C: real agent turns ------------------------------------------------------
PROMPT='Run `sh probe.sh` in the current directory once and reply with its exact output. It is a local connectivity check.'
if [ "${SP4B_RUN_AGENTS:-0}" = 1 ]; then
  fingerprint > "$OUT/fingerprint.before"
  touch "$OUT/marker"

  start_server C-claude-sandbox-default
  (cd "$REPO" && claude -p "$PROMPT" \
      --settings '{"sandbox":{"enabled":true}}' \
      --setting-sources project,local --strict-mcp-config --no-session-persistence \
      --permission-prompts none --model haiku --max-turns 6 \
      --output-format stream-json --verbose) > "$OUT/C-claude-sandbox-default.stream.jsonl" 2>"$OUT/C-claude.err" || true
  stop_server

  # NOTE: `codex exec` (0.159.2) persists [projects."<repo>"] trust_level = "trusted" into
  # ~/.codex/config.toml. Set SP4B_CODEX_UNTRUST=1 to remove exactly that block afterwards
  # (scripts/codex_untrust.py); otherwise the script only reports it.
  CODEX_COMMON='--ephemeral --json -m gpt-5.4-mini -c model_reasoning_effort="low" -c notify=[] --disable hooks'
  for spec in \
    "read-only|-s read-only" \
    "workspace-write|-s workspace-write" \
    "agent|-s workspace-write -c approval_policy=\"on-request\""; do
    name=${spec%%|*}; args=${spec#*|}
    start_server "C-codex-$name"
    # shellcheck disable=SC2086
    (cd "$REPO" && eval codex exec $CODEX_COMMON $args -C "\"$REPO\"" "\"\$PROMPT\"") \
      > "$OUT/C-codex-$name.stream.jsonl" 2>"$OUT/C-codex-$name.err" </dev/null || true
    stop_server
  done
  if [ "${SP4B_CODEX_UNTRUST:-0}" = 1 ]; then
    python3 -I "$SCRIPTS/codex_untrust.py" "$HOME/.codex/config.toml" "$REPO" \
      "$(grep config.toml "$OUT/fingerprint.before" | cut -c1-16)" > "$OUT/C-untrust.txt"
  fi

  fingerprint > "$OUT/fingerprint.after"
  {
    echo "config fingerprints identical before/after: $(cmp -s "$OUT/fingerprint.before" "$OUT/fingerprint.after" && echo yes || echo NO)"
    diff "$OUT/fingerprint.before" "$OUT/fingerprint.after" | sed -n 's/^> [0-9a-f]* [0-9]* /changed: /p' || true
    echo "codex trust entry for the scratch repo present: $(grep -c "sp4b" "$HOME/.codex/config.toml" || true)"
    echo "entries under ~/.claude and ~/.codex created during the runs that belong to this spike:"
    find "$HOME/.claude" "$HOME/.codex" -newer "$OUT/marker" 2>/dev/null | grep "sp4b" | sed "s#$HOME#\$HOME#" | sort
    echo "Codex state/log files touched during the runs (shared with other sessions):"
    find "$HOME/.codex" -maxdepth 1 -newer "$OUT/marker" -type f 2>/dev/null | sed "s#$HOME#\$HOME#" | sort
  } > "$OUT/C-footprint.txt"
fi

python3 -I "$SCRIPTS/agent_summary.py" "$OUT" > "$OUT/summary.txt"
rm -rf "$CACHE_DIR"
for f in "$OUT"/summary.txt "$OUT"/B-codex-runner.txt "$OUT"/C-footprint.txt; do
  [ -f "$f" ] && sanitize < "$f" > "$RESULTS/04-$(basename "$f")"
done
cat "$RESULTS/04-summary.txt"
