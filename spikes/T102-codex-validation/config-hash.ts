// Before/after fingerprint of ~/.codex/config.toml and ~/.codex/hooks.json
// (brief: "运行前后做哈希对比"). Nothing else under ~/.codex is read.
//
//   node config-hash.ts snapshot <out.json> <tmp dir of the run>
//   node config-hash.ts compare <before.json> <after.json>
//
// The snapshot (0600, outside the repository) holds SHA-256 digests only: file,
// TOML tables, top-level keys of hooks.json, `[projects]` entries by digest of the
// path plus a flag for the run's temp dir. `compare` names top-level tables only;
// sub-tables (projects, MCP servers, providers) appear as counts, values never.

import { createHash } from "node:crypto";
import { existsSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";

type Json = any;
const sha = (s: string) => createHash("sha256").update(s).digest("hex");
const KEY = /\s*("(?:[^"\\]|\\.)*"|'[^']*'|[^.\s=]+)\s*(?:\.|$)/g;
/** Dotted TOML key -> parts: a."b.c".d -> [a, b.c, d]. */
const keyParts = (s: string) => [...s.matchAll(KEY)].map((m) => (m[1].startsWith('"') ? JSON.parse(m[1]) : m[1].replace(/^'|'$/g, "")));

/** Raw text per table ("" is the root) and per `[projects]` entry, by path. Line-based, enough for digests. */
function splitToml(text: string) {
  const tables: Record<string, string> = {};
  const projects: Record<string, string> = {};
  let cur: string[] = [];
  let multi: string | null = null;
  for (const line of text.replace(/\n$/, "").split("\n")) {
    const h = multi ? null : line.match(/^\s*\[\[?\s*(.+?)\s*\]\]?\s*(#.*)?$/);
    if (h) cur = keyParts(h[1]);
    const kv = !h && !multi && cur.join(".") === "projects" ? line.match(/^\s*(.+?)\s*=/) : null;
    const project = cur[0] === "projects" && cur.length > 1 ? cur.slice(1).join(".") : kv ? keyParts(kv[1])[0] : null;
    if (project) projects[project] = (projects[project] ?? "") + line + "\n";
    tables[cur.join(".")] = (tables[cur.join(".")] ?? "") + line + "\n";
    for (const q of line.match(/"""|'''/g) ?? []) multi = multi === q ? null : (multi ?? q);
  }
  return { tables, projects };
}

function snapshot(out: string, base: string) {
  const dir = join(homedir(), ".codex");
  const bases = [base, realpathSync(base)].flatMap((p) => [p, p.replace(/^\/private/, "")]);
  const snap: Json = {};
  if (existsSync(join(dir, "config.toml"))) {
    const text = readFileSync(join(dir, "config.toml"), "utf8");
    const { tables, projects } = splitToml(text);
    const proj = Object.entries(projects).map(([k, v]) => [sha(k), { digest: sha(v), inRunTempDir: bases.some((p) => k.startsWith(p)) }]);
    snap["config.toml"] = { file: sha(text), parts: Object.fromEntries(Object.entries(tables).map(([k, v]) => [k, sha(v)])), projects: Object.fromEntries(proj) };
  }
  if (existsSync(join(dir, "hooks.json"))) {
    const text = readFileSync(join(dir, "hooks.json"), "utf8");
    let parts: Json = {};
    try {
      parts = Object.fromEntries(Object.entries(JSON.parse(text)).map(([k, v]) => [k, sha(JSON.stringify(v))]));
    } catch {}
    snap["hooks.json"] = { file: sha(text), parts };
  }
  writeFileSync(out, JSON.stringify(snap, null, 1), { mode: 0o600 });
  console.log(`snapshot: ${Object.keys(snap).join(", ") || "no files"}`);
}

/** Changed / unchanged per top-level name; sub-tables grouped as "<top>.*" with counts only. */
function diff(before: Record<string, string>, after: Record<string, string>) {
  const rows = new Map<string, number[]>(); // [changed, added, removed, total]
  for (const k of new Set([...Object.keys(before), ...Object.keys(after)])) {
    const top = k === "" ? "(root)" : k.includes(".") ? `${k.split(".")[0]}.*` : k;
    const r = rows.get(top) ?? [0, 0, 0, 0];
    const i = !(k in before) ? 1 : !(k in after) ? 2 : before[k] !== after[k] ? 0 : -1;
    if (i >= 0) r[i]++;
    r[3]++;
    rows.set(top, r);
  }
  return [...rows].map(([k, [ch, add, rm, n]]) => `  ${k}: ${ch + add + rm ? `CHANGED (${ch} changed, ${add} added, ${rm} removed of ${n})` : "unchanged"}`);
}

function compare(x: Json, y: Json) {
  for (const f of ["config.toml", "hooks.json"]) {
    const [p, q] = [x[f], y[f]];
    console.log(`${f}: ${!p && !q ? "absent before and after" : !p ? "created" : !q ? "removed" : p.file === q.file ? "unchanged" : "CHANGED"}`);
    if (!p || !q || p.file === q.file) continue;
    console.log(diff(p.parts, q.parts).join("\n"));
    if (f !== "config.toml") continue;
    const added = Object.keys(q.projects).filter((k) => !(k in p.projects));
    console.log(`  [projects] entries added: ${added.length}, of them in this run's temp dir: ${added.filter((k) => q.projects[k].inRunTempDir).length}`);
  }
}

const [cmd, a, b] = process.argv.slice(2);
if (cmd === "snapshot") snapshot(a, b);
else if (cmd === "compare") compare(JSON.parse(readFileSync(a, "utf8")), JSON.parse(readFileSync(b, "utf8")));
else throw new Error("usage: snapshot <out> <tmp dir> | compare <before> <after>");
