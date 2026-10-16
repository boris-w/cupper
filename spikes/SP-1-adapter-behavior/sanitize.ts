// Redact raw recordings before they are committed.
//
//   node sanitize.ts --in <base>/raw --out recordings --base <base> [--terms <file>]
//
// Everything machine- or person-specific is taken from the running environment
// (HOME, USER, TMPDIR, host names) or from a local terms file, so this script
// itself contains no personal data. After writing, every output file is scanned
// again and the script exits non-zero if anything suspicious is left.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { hostname } from "node:os";
import { join } from "node:path";

type Json = any;

function arg(name: string, fallback?: string): string {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  if (fallback !== undefined) return fallback;
  throw new Error(`missing --${name}`);
}

const inDir = arg("in");
const outDir = arg("out");
const base = arg("base");
const termsFile = arg("terms", "");

function escapeRe(s: string) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}
function variants(p: string): string[] {
  const out = new Set([p.replace(/\/+$/, "")]);
  try {
    out.add(realpathSync(p).replace(/\/+$/, ""));
  } catch {}
  for (const v of [...out]) {
    if (v.startsWith("/private/")) out.add(v.slice("/private".length));
    else out.add("/private" + v);
  }
  return [...out].sort((a, b) => b.length - a.length);
}
function scutil(key: string): string {
  try {
    return execFileSync("scutil", ["--get", key], { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

const home = process.env.HOME!;
const user = process.env.USER ?? "";
const tmp = process.env.TMPDIR ?? "";
const hosts = [...new Set([hostname(), hostname().split(".")[0], scutil("ComputerName"), scutil("LocalHostName")].filter((h) => h.length > 2))];
const terms = termsFile && existsSync(termsFile)
  ? readFileSync(termsFile, "utf8").split("\n").map((s) => s.trim()).filter(Boolean)
  : [];

/** Claude Code names per-project directories after the path with every non-alphanumeric replaced by "-". */
const encoded = (p: string) => p.replace(/[^A-Za-z0-9]/g, "-");
// Machine-specific path segments (for example the random part of TMPDIR).
const tmpSegments = tmp.split("/").filter((s) => s.length >= 12);

// Ordered: longest, most specific paths first.
const rules: Array<[RegExp, string]> = [];
for (const v of variants(base)) rules.push([new RegExp(escapeRe(v), "g"), "<tmp>"]);
for (const v of variants(base)) rules.push([new RegExp(escapeRe(encoded(v)), "g"), "-tmp-"]);
if (tmp) for (const v of variants(tmp)) rules.push([new RegExp(escapeRe(v) + "/?", "g"), "<tmpdir>/"]);
if (tmp) for (const v of variants(tmp)) rules.push([new RegExp(escapeRe(encoded(v)), "g"), "-tmpdir"]);
for (const v of variants(home)) rules.push([new RegExp(escapeRe(v), "g"), "$HOME"]);
for (const v of variants(home)) rules.push([new RegExp(escapeRe(encoded(v)), "g"), "-HOME"]);
for (const s of tmpSegments) rules.push([new RegExp(escapeRe(s), "g"), "<tmpid>"]);
for (const h of hosts) rules.push([new RegExp(escapeRe(h), "gi"), "<host>"]);
for (const t of terms) rules.push([new RegExp(escapeRe(t), "gi"), "<redacted>"]);
if (user.length > 2) rules.push([new RegExp(`\\b${escapeRe(user)}\\b`, "gi"), "<user>"]);
rules.push([/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, "<email>"]);
rules.push([/\bsk-[A-Za-z0-9_-]{16,}/g, "<secret>"]);
rules.push([/\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g, "<secret>"]);
rules.push([/\b(Bearer|bearer)\s+[A-Za-z0-9._~+/=-]{8,}/g, "$1 <secret>"]);

function scrubString(s: string): string {
  let out = s;
  for (const [re, rep] of rules) out = out.replace(re, rep);
  // Long base64/hex runs (keys, signatures, hashes). Ids such as UUIDs or
  // "rs_<hex>" message ids are shorter or broken up by "-" / "_", so they survive.
  out = out.replace(/[A-Za-z0-9+/=]{64,}/g, (m) => (/[0-9]/.test(m) && /[A-Za-z]/.test(m) && !m.includes("//") ? "<opaque>" : m));
  return out;
}

/** Recursive scrub. `parent` lets image payloads (our own test PNG) pass untouched. */
function scrub(v: Json, parent?: Json): Json {
  if (typeof v === "string") return scrubString(v);
  if (Array.isArray(v)) return v.map((x) => scrub(x, v));
  if (v && typeof v === "object") {
    const out: Json = {};
    for (const [k, x] of Object.entries(v)) {
      if (k === "data" && typeof x === "string" && typeof v.mimeType === "string" && v.mimeType.startsWith("image/")) out[k] = x;
      else if (k === "signature" && typeof x === "string") out[k] = "<signature>";
      else out[k] = scrub(x, v);
    }
    return out;
  }
  return v;
}

// ---- structural rules -------------------------------------------------------

/** User-installed skills and commands reveal personal tooling: keep the count, drop the names. */
function redactCommands(cmds: Json[]): Json[] {
  let n = 0;
  return cmds.map((c) => {
    const d = typeof c.description === "string" ? c.description : "";
    const userSourced = /\((user|project)\)\s*$/.test(d) || String(c.name).startsWith("$");
    if (!userSourced) return c;
    n++;
    return { ...c, name: String(c.name).startsWith("$") ? `$user-skill-${n}` : `user-skill-${n}`, description: "<redacted user skill>" };
  });
}

/** Keep public gpt-* models only; the rest of a private gateway catalog is dropped. */
const keepModel = (id: unknown) => typeof id === "string" && /^(gpt-|claude|opus|sonnet|haiku|default)/i.test(id);
function redactModels(obj: Json) {
  if (Array.isArray(obj?.availableModels)) obj.availableModels = obj.availableModels.filter((m: Json) => keepModel(m.modelId));
}
function redactConfigOptions(opts: Json[]) {
  for (const o of opts ?? []) {
    if (o?.category === "model" && Array.isArray(o.options)) {
      o.options = o.options.filter((x: Json) => keepModel(x.value) || Array.isArray(x.options));
    }
  }
}

/** Process labels keep only what identifies the role; anything else is a user-configured tool. */
function procLabel(label: string): string {
  if (/^node index\.js/.test(label)) return "adapter (node)";
  if (/^claude auth status/.test(label)) return "claude auth status";
  if (/^claude\b/.test(label)) return "claude CLI";
  if (/^node codex(\.js)? app-server/.test(label)) return "codex wrapper (node)";
  if (/^codex app-server/.test(label)) return "codex app-server";
  const m = label.match(/^(sleep|sh|zsh|bash|git)\b/);
  if (m) return m[1];
  return "user-configured tool or MCP server";
}

function structural(f: Json): Json {
  const m = f.msg;
  if (f.dir === "note" && Array.isArray(f.data?.procs)) {
    f.data.procs = f.data.procs.map((p: Json) => ({ ...p, label: procLabel(p.label) }));
  }
  if (f.dir === "note" && f.data?.tree?.procs) {
    f.data.tree.procs = f.data.tree.procs.map((p: Json) => ({ ...p, label: procLabel(p.label) }));
  }
  if (f.dir === "note" && f.data?.victim?.label) f.data.victim.label = procLabel(f.data.victim.label);
  if (!m) return f;
  const u = m.params?.update;
  if (u?.sessionUpdate === "available_commands_update" && Array.isArray(u.availableCommands)) {
    u.availableCommands = redactCommands(u.availableCommands);
  }
  if (u?.sessionUpdate === "config_option_update") redactConfigOptions(u.configOptions);
  if (m.result) {
    if (m.result.models) redactModels(m.result.models);
    if (Array.isArray(m.result.configOptions)) redactConfigOptions(m.result.configOptions);
  }
  if (m.method === "_auth/status_update" && m.params?.authStatus?.account) {
    const a = m.params.authStatus.account;
    for (const k of Object.keys(a)) if (k !== "plan") a[k] = `<${k}>`;
  }
  return f;
}

// ---- run ----------------------------------------------------------------------

const leftovers: string[] = [];
const scanTerms = [home, encoded(home), "/Users/", "/var/folders", "/private/var", "-var-folders-", ...tmpSegments, ...hosts, ...terms];
if (user.length > 2) scanTerms.push(user);

let files = 0;
for (const adapter of readdirSync(inDir)) {
  const src = join(inDir, adapter);
  if (!existsSync(src) || !readdirSync(src).length) continue;
  const dst = join(outDir, adapter);
  mkdirSync(dst, { recursive: true });
  for (const name of readdirSync(src).filter((n) => n.endsWith(".jsonl")).sort()) {
    const lines = readFileSync(join(src, name), "utf8").split("\n").filter(Boolean);
    const out = lines.map((l) => JSON.stringify(scrub(structural(JSON.parse(l)))));
    const body = out.join("\n") + "\n";
    writeFileSync(join(dst, name), body);
    files++;
    const lower = body.toLowerCase();
    for (const t of scanTerms) if (t && lower.includes(t.toLowerCase())) leftovers.push(`${adapter}/${name}: ${t === user ? "<user>" : t === home ? "<home>" : "<term>"}`);
    if (/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/.test(body)) leftovers.push(`${adapter}/${name}: email-like`);
  }
}
console.log(`sanitized ${files} files into ${outDir}`);
if (leftovers.length) {
  console.log("LEFTOVERS:\n" + [...new Set(leftovers)].join("\n"));
  process.exit(1);
}
console.log("scan clean");
