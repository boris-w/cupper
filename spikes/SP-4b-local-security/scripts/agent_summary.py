"""Summarize item 4: which probes reached the server, and what each agent did.

Reads <dir>/*.server.jsonl (ground truth) and <dir>/C-*.stream.jsonl (agent event streams).
Only commands, outputs and short messages are printed; no prompts or account data.
"""

import glob
import json
import os
import sys


def clip(s, n=400):
    s = s if isinstance(s, str) else json.dumps(s, ensure_ascii=False)
    return s if len(s) <= n else s[:n] + "...(clipped)"


def server_view(path):
    rows = []
    for line in open(path):
        if not line.strip():
            continue
        r = json.loads(line)
        if r.get("msg_index") != 1:
            continue
        chain = [p["comm"] for p in (r.get("lineage") or {}).get("chain", [])][:6]
        rows.append(f"  connected: {r.get('msg')!r} via {r['socket'].rsplit('/', 2)[-2]}  chain={'<'.join(chain)}")
    return rows or ["  connected: (none)"]


def claude_view(path):
    out = []
    for line in open(path):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        t = e.get("type")
        if t == "system" and e.get("subtype") == "init":
            out.append(f"  init: model={e.get('model')} permissionMode={e.get('permissionMode')}")
        elif t == "assistant":
            for c in e.get("message", {}).get("content", []):
                if c.get("type") == "tool_use":
                    out.append(f"  tool_use {c.get('name')}: {clip(c.get('input'), 300)}")
        elif t == "user":
            for c in e.get("message", {}).get("content", []) if isinstance(e.get("message", {}).get("content"), list) else []:
                if c.get("type") == "tool_result":
                    content = c.get("content")
                    if isinstance(content, list):
                        content = " ".join(x.get("text", "") for x in content if isinstance(x, dict))
                    out.append(f"  tool_result (is_error={c.get('is_error')}): {clip(content)}")
        elif t == "result":
            out.append(f"  result: turns={e.get('num_turns')} permission_denials={clip(e.get('permission_denials'), 300)}")
            out.append(f"  final: {clip(e.get('result'), 500)}")
    return out


def codex_view(path):
    out = []
    for line in open(path):
        try:
            e = json.loads(line)
        except ValueError:
            continue
        item = e.get("item") or {}
        t = e.get("type", "")
        it = item.get("type", "")
        if t == "item.completed" and it == "command_execution":
            out.append(f"  command: {clip(item.get('command'), 200)} exit={item.get('exit_code')} status={item.get('status')}")
            out.append(f"    output: {clip(item.get('aggregated_output', ''), 600)}")
        elif t == "item.completed" and it == "agent_message":
            out.append(f"  agent_message: {clip(item.get('text', ''), 500)}")
        elif "approv" in line or "escalat" in line:
            out.append(f"  event {t}/{it}: {clip(e, 400)}")
        elif t in ("error", "turn.failed"):
            out.append(f"  {t}: {clip(e, 400)}")
    return out


def main(d):
    for f in sorted(glob.glob(os.path.join(d, "*.server.jsonl"))):
        name = os.path.basename(f).replace(".server.jsonl", "")
        print(f"\n### {name}")
        print("\n".join(server_view(f)))
        stream = os.path.join(d, name + ".stream.jsonl")
        if os.path.exists(stream):
            view = claude_view(stream) if "claude" in name else codex_view(stream)
            print("\n".join(view) if view else "  (no events parsed)")


main(sys.argv[1])
