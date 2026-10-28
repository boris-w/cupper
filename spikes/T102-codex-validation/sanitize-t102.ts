// Pass 0 of the sanitizing, before SP-1's sanitize.ts and T003's sanitize-t003.ts,
// for two Codex shapes they do not know: the names of the user's MCP servers
// (taken from the recordings' MCP tool calls: `rawInput.server`, titles
// `mcp.<server>.<tool>`) become `<redacted>` wherever they occur, in any spelling
// (also camelCase inside `_meta` keys); `skillPath` (sent to AIR clients only)
// carries the local path of each installed skill or plugin: a placeholder.
//
//   node sanitize-t102.ts <stage0 dir>     (rewrites its *.jsonl files in place)

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
const files = readdirSync(dir, { recursive: true }).map(String).filter((f) => f.endsWith(".jsonl")).map((f) => join(dir, f));
const names = new Set<string>();
for (const f of files) {
  for (const line of readFileSync(f, "utf8").split("\n").filter(Boolean)) {
    const u = JSON.parse(line).msg?.params?.update;
    if (u?._meta?.is_mcp_tool_call && typeof u.rawInput?.server === "string") names.add(u.rawInput.server);
    const t = /^mcp\.([^.]+)\./.exec(typeof u?.title === "string" ? u.title : "");
    if (t) names.add(t[1]);
  }
}
// Every spelling: snake, kebab, camel, spaced or run together, any case (foo_bar ~ fooBar ~ FOO-BAR).
const spelling = (n: string) => n.split(/[^A-Za-z0-9]+|(?<=[a-z0-9])(?=[A-Z])/).filter(Boolean).join("[\\s_.-]?");
const re = names.size ? new RegExp([...names].map(spelling).sort((a, b) => b.length - a.length).join("|"), "gi") : null;
for (const f of files) {
  const body = readFileSync(f, "utf8").replace(/("skillPath":)"(?:[^"\\]|\\.)*"/g, '$1"<redacted skill path>"');
  writeFileSync(f, re ? body.replace(re, "<redacted>") : body);
}
console.log(`${files.length} files: ${names.size} MCP server name(s) redacted, skill paths replaced`);
