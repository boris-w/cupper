// Print a compact timeline of a recording: node inspect.ts <file.jsonl> [--full <substring>]
import { readFileSync } from "node:fs";

const file = process.argv[2];
const fullIdx = process.argv.indexOf("--full");
const fullFilter = fullIdx > 0 ? process.argv[fullIdx + 1] : undefined;
const width = 160;

function short(v: unknown, n = 110): string {
  const s = typeof v === "string" ? v : JSON.stringify(v);
  if (s === undefined) return "";
  return s.length > n ? s.slice(0, n) + "…" : s;
}

for (const line of readFileSync(file, "utf8").split("\n")) {
  if (!line.trim()) continue;
  const f = JSON.parse(line);
  const t = String(f.t).padStart(7);
  let out = "";
  if (f.dir === "note") {
    out = `${t} NOTE ${f.note} ${short(f.data, 130)}`;
  } else if (f.dir === "recv-nonjson") {
    out = `${t} NONJSON ${short(f.line)}`;
  } else {
    const m = f.msg;
    const arrow = f.dir === "send" ? "->" : "<-";
    if (m.method === "session/update") {
      const u = m.params.update;
      const k = u.sessionUpdate;
      let info = "";
      if (k === "agent_message_chunk" || k === "agent_thought_chunk" || k === "user_message_chunk") info = short(u.content?.text ?? u.content, 80);
      else if (k === "tool_call" || k === "tool_call_update") info = `${u.toolCallId} ${u.status ?? ""} ${u.kind ?? ""} ${short(u.title ?? "", 50)} content=${(u.content ?? []).map((c: any) => c.type).join(",")} meta=${Object.keys(u._meta ?? {}).join(",")}`;
      else info = short(u, 100);
      out = `${t} ${arrow} update ${k} ${info}`;
    } else if (m.method) {
      out = `${t} ${arrow} ${m.id !== undefined ? `#${m.id} ` : ""}${m.method} ${short(m.params, 100)}`;
    } else {
      out = `${t} ${arrow} #${m.id} ${m.error ? "ERROR " + short(m.error, 120) : "result " + short(m.result, 120)}`;
    }
  }
  console.log(out.slice(0, width));
  if (fullFilter && line.includes(fullFilter)) console.log(JSON.stringify(f, null, 1));
}
