// Second sanitizing pass, run after SP-1's sanitize.ts (same rules for paths,
// user name, host names, local terms, e-mail addresses, keys and account
// fields). It adds what T003 needs on top:
//
// - session IDs become `session-01`, `session-02`, ... everywhere they occur
//   (the brief asks for no session IDs in the repository);
// - slash commands synced from claude.ai or provided by plugins are redacted
//   like user skills (SP-1 only knew `(user)` and `(project)`);
// - process labels anywhere in notes and result files get SP-1's role-only
//   labels (SP-1 only mapped a few known keys);
// - result files, which SP-1's script only reads when wrapped as a one-line
//   JSONL note (`{"dir":"note","note":"results","data":...}`), are unwrapped.
//
//   node sanitize-t003.ts --in <SP-1 sanitize output> --out <dir> [--terms <file>]
//
// Afterwards every output file is scanned again; leftovers exit non-zero.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { hostname } from "node:os";
import { join } from "node:path";
import { arg } from "./common.ts";

type Json = any;

const inDir = arg("in");
const outDir = arg("out");
const termsFile = arg("terms", "");

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const aliases = new Map<string, string>();

function collectSessionIds(v: Json) {
  if (Array.isArray(v)) return v.forEach(collectSessionIds);
  if (!v || typeof v !== "object") return;
  for (const [k, x] of Object.entries(v)) {
    if (k === "sessionId" && typeof x === "string" && UUID.test(x) && !aliases.has(x)) {
      aliases.set(x, `session-${String(aliases.size + 1).padStart(2, "0")}`);
    }
    collectSessionIds(x);
  }
}

function replaceIds(s: string): string {
  let out = s;
  for (const [id, a] of aliases) out = out.split(id).join(a);
  return out;
}

// Same role mapping as SP-1's sanitize.ts. Labels that pass 1 already mapped stay as they are.
const ROLE_LABELS = new Set(["adapter (node)", "claude auth status", "claude CLI", "codex wrapper (node)", "codex app-server", "user-configured tool or MCP server"]);
function procLabel(label: string): string {
  if (ROLE_LABELS.has(label)) return label;
  if (/^node index\.js/.test(label)) return "adapter (node)";
  if (/^claude auth status/.test(label)) return "claude auth status";
  if (/^claude\b/.test(label)) return "claude CLI";
  const m = label.match(/^(sleep|sh|zsh|bash|git)\b/);
  if (m) return m[1];
  return "user-configured tool or MCP server";
}

function relabel(v: Json): Json {
  if (Array.isArray(v)) return v.map(relabel);
  if (!v || typeof v !== "object") return v;
  const out: Json = {};
  const isProc = "role" in v || "pid" in v;
  for (const [k, x] of Object.entries(v)) out[k] = k === "label" && typeof x === "string" && isProc ? procLabel(x) : relabel(x);
  return out;
}

let redacted = 0;
/** Numbered per list, as in SP-1, so a skill keeps its placeholder across updates. */
function redactCommands(f: Json) {
  const u = f?.msg?.params?.update;
  if (u?.sessionUpdate !== "available_commands_update" || !Array.isArray(u.availableCommands)) return;
  let skill = 0;
  u.availableCommands = u.availableCommands.map((c: Json) => {
    const d = typeof c.description === "string" ? c.description : "";
    if (/\(claude\.ai sync\)\s*$/.test(d) || String(c.name).includes(":")) {
      skill++;
      redacted++;
      return { ...c, name: `synced-or-plugin-skill-${skill}`, description: "<redacted synced or plugin skill>" };
    }
    return c;
  });
}

// ---- run ----------------------------------------------------------------------

const files: Array<{ sub: string; name: string; lines: Json[] }> = [];
for (const sub of readdirSync(inDir)) {
  for (const name of readdirSync(join(inDir, sub)).filter((n) => n.endsWith(".jsonl")).sort()) {
    const lines = readFileSync(join(inDir, sub, name), "utf8").split("\n").filter(Boolean).map((l) => JSON.parse(l));
    lines.forEach(collectSessionIds);
    files.push({ sub, name, lines });
  }
}

const written: string[] = [];
for (const { sub, name, lines } of files) {
  mkdirSync(join(outDir, sub), { recursive: true });
  if (sub === "results") {
    const data = relabel(lines[0].data);
    const p = join(outDir, sub, name.replace(/\.jsonl$/, ".json"));
    writeFileSync(p, replaceIds(JSON.stringify(data, null, 1)) + "\n");
    written.push(p);
    continue;
  }
  lines.forEach(redactCommands);
  // SP-1's client also records process lists under keys its sanitizer does not
  // map (for example `procsAtExit`), so every note gets the role-only labels.
  for (const l of lines) if (l.dir === "note" && l.data) l.data = relabel(l.data);
  const p = join(outDir, sub, name);
  writeFileSync(p, lines.map((l) => replaceIds(JSON.stringify(l))).join("\n") + "\n");
  written.push(p);
}

// ---- scan ---------------------------------------------------------------------

function scutil(key: string): string {
  try {
    return execFileSync("scutil", ["--get", key], { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}
const home = process.env.HOME ?? "";
const user = process.env.USER ?? "";
const tmpSegments = (process.env.TMPDIR ?? "").split("/").filter((s) => s.length >= 12);
const hosts = [hostname(), hostname().split(".")[0], scutil("ComputerName"), scutil("LocalHostName")].filter((h) => h.length > 2);
const terms = termsFile && existsSync(termsFile) ? readFileSync(termsFile, "utf8").split("\n").map((s) => s.trim()).filter(Boolean) : [];
const scan = [home, "/Users/", "/var/folders", "/private/var", "-var-folders-", ...tmpSegments, ...hosts, ...terms, ...aliases.keys()].filter(Boolean);
if (user.length > 2) scan.push(user);

const leftovers: string[] = [];
for (const p of written) {
  const body = readFileSync(p, "utf8").toLowerCase();
  for (const t of scan) if (body.includes(t.toLowerCase())) leftovers.push(`${p.slice(outDir.length + 1)}: ${aliases.has(t) ? "<session id>" : t === user ? "<user>" : "<term>"}`);
  if (/[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/.test(body)) leftovers.push(`${p.slice(outDir.length + 1)}: email-like`);
}
console.log(`wrote ${written.length} files; ${aliases.size} session ids aliased; ${redacted} synced or plugin skill entries redacted`);
if (leftovers.length) {
  console.log("LEFTOVERS:\n" + [...new Set(leftovers)].join("\n"));
  process.exit(1);
}
console.log("scan clean");
