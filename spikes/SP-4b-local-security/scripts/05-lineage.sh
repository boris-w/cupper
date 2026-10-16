#!/bin/sh
# Item 5: lineage signals vs common ways a command detaches from its parent.
# `launchctl submit` is deliberately NOT exercised (it registers a launchd job); see README.
# Usage: sh scripts/05-lineage.sh
. "$(dirname "$0")/common.sh"
build

W=$SP4B_WORK/lineage
OUT=$LOGS/05
rm -rf "$W" "$OUT"; mkdir -p "$W" "$OUT"
S=$W/host.sock

cat > "$W/client.py" <<'EOF'
import socket, sys, time
time.sleep(1.0)  # let intermediate parents exit first
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
s.sendall((sys.argv[2] + "\n").encode())
s.recv(10)
EOF

# The stand-in adapter: each line detaches the client in a different way.
cat > "$W/adapter.sh" <<'EOF'
#!/bin/sh
S=$1; CL="/usr/bin/python3 $2"
$CL "$S" direct-child
$CL "$S" background-child-of-adapter &
sh -c "$CL '$S' orphan-after-parent-exit &"
sh -c "nohup $CL '$S' nohup >/dev/null 2>&1 &"
( ( $CL "$S" double-fork & ) & )
/bin/bash -c "$CL '$S' bash-disown & disown; exit 0"
/bin/zsh -c "$CL '$S' zsh-disown &!"
/usr/bin/perl -MPOSIX -e 'fork and exit; POSIX::setsid(); exec @ARGV' /usr/bin/python3 "$2" "$S" fork-setsid
wait
sleep 2.5
EOF
chmod +x "$W/adapter.sh"

for mode in plain disclaim; do
  flag=""; [ "$mode" = disclaim ] && flag=--disclaim
  rm -f "$S"
  # shellcheck disable=SC2086
  "$BIN/lineage-host" --socket "$S" $flag --log "$OUT/$mode.jsonl" -- /bin/sh "$W/adapter.sh" "$S" "$W/client.py" >/dev/null 2>"$OUT/$mode.err" &
  LH=$!
  wait_for_socket "$S"
  /usr/bin/python3 "$W/client.py" "$S" control-not-a-descendant &
  wait "$LH" || true
  wait
done

python3 -I - "$OUT" > "$OUT/summary.txt" <<'EOF'
import json, os, sys
d = sys.argv[1]
for mode in ("plain", "disclaim"):
    rows = [json.loads(l) for l in open(os.path.join(d, mode + ".jsonl")) if l.strip()]
    ad = rows[0]["adapter"]
    print(f"\n### adapter spawned with POSIX_SPAWN_SETSID, responsibility disclaimed: {ad['disclaimed']}")
    print("label | ancestor_chain | same_pgid | same_sid | same_responsible_pid | detected | chain")
    for r in sorted(rows[1:], key=lambda r: r["label"]):
        s = r["signals"]
        chain = "<".join(p["comm"] for p in r["chain"][:5])
        print(f"{r['label']} | {s['ancestor_chain']} | {s['same_pgid']} | {s['same_sid']} | "
              f"{s['same_responsible_pid']} | {r['detected_by_any']} | {chain}")
EOF
sanitize < "$OUT/summary.txt" > "$RESULTS/05-lineage.txt"
cat "$RESULTS/05-lineage.txt"
