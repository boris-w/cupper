/**
 * Redaction of recordings (format v1, design 5.9.3) before they enter
 * `testdata/`, migrated from SP-1's `sanitize.ts` with its rules unchanged.
 * Everything machine- or person-specific comes from the machine the tool runs
 * on (home, user, temporary directory, host names), the temporary roots of
 * the recorded run and a local terms file, so this code holds no personal
 * data. Session IDs become `session-NN`; the names of the user's MCP servers,
 * taken from where Codex shows them, become `<redacted>` wherever they occur.
 * A text streamed over several frames is redacted once more, folded.
 * Running it on its own output changes nothing.
 */
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, realpathSync, writeFileSync } from "node:fs";
import { hostname } from "node:os";
import { dirname, join } from "node:path";

import { either, escapeRe, expand, hits, patterns, scan, terms } from "./scan.ts";
import type { Finding, Matcher } from "./scan.ts";
import { fold, streams } from "./streams.ts";

// eslint-disable-next-line @typescript-eslint/no-explicit-any -- recordings are free-form JSON
type Json = any;

export interface Machine {
  home: string;
  user: string;
  tmp: string;
  hosts: string[];
  /** Temporary roots of the recorded run, written as `<tmp>`. */
  bases: string[];
  /** Local terms such as organization names, written as `<redacted>`. */
  terms: string[];
}

export function currentMachine(bases: string[], localTerms: string[]): Machine {
  const name = hostname();
  const hosts = [name, name.split(".")[0] ?? "", scutil("ComputerName"), scutil("LocalHostName")];
  return {
    home: process.env.HOME ?? "",
    user: process.env.USER ?? "",
    tmp: process.env.TMPDIR ?? "",
    hosts: [...new Set(hosts.filter((h) => h.length > 2))],
    bases,
    terms: localTerms,
  };
}

function scutil(key: string): string {
  try {
    return execFileSync("scutil", ["--get", key], { encoding: "utf8" }).trim();
  } catch {
    return "";
  }
}

function variants(p: string): string[] {
  const out = new Set([p.replace(/\/+$/, "")]);
  try {
    out.add(realpathSync(p).replace(/\/+$/, ""));
  } catch {
    // A path of another machine: only its spelling is known.
  }
  for (const v of [...out]) {
    if (v.startsWith("/private/")) out.add(v.slice("/private".length));
    else out.add("/private" + v);
  }
  return [...out].sort((a, b) => b.length - a.length);
}

/** Claude Code names per-project directories after the path with every non-alphanumeric replaced by "-". */
const encoded = (p: string) => p.replace(/[^A-Za-z0-9]/g, "-");

/** Machine-specific path segments (for example the random part of TMPDIR). */
const tmpSegments = (m: Machine) => m.tmp.split("/").filter((s) => s.length >= 12);

/** A rule: what to replace, with what (`$1` names a group). */
type Rule = [RegExp, string | ((match: string) => string)];

const replace = (s: string, [re, rep]: Rule) =>
  typeof rep === "string" ? s.replace(re, rep) : s.replace(re, rep);

/**
 * A rule applied to a text split into pieces, keeping the number of pieces:
 * the replacement goes where its match starts, and the rest of the match is
 * cut from the pieces it runs into.
 */
function replacePieces(pieces: string[], [re, rep]: Rule): string[] {
  const { text, owner } = fold(pieces);
  const matches = [...text.matchAll(re)];
  if (!matches.length) return pieces;
  const out: string[][] = pieces.map(() => []);
  let at = 0;
  const copy = (to: number) => {
    for (; at < to; at++) out[owner[at] ?? 0]?.push(text.charAt(at));
  };
  for (const m of matches) {
    copy(m.index);
    const by = typeof rep === "string" ? rep.replace(/\$(\d)/g, (_, n) => m[n] ?? "") : rep(m[0]);
    out[owner[m.index] ?? pieces.length - 1]?.push(by);
    at = m.index + m[0].length;
  }
  copy(text.length);
  return out.map((parts) => parts.join(""));
}

/** Ordered: longest, most specific paths first. */
function rules(m: Machine): Rule[] {
  const rules: Rule[] = [];
  const add = (paths: string[], rep: string, map = (p: string) => p, tail = "") => {
    for (const p of paths.filter(Boolean).flatMap(variants)) {
      rules.push([new RegExp(escapeRe(map(p)) + tail, "g"), rep]);
    }
  };
  add(m.bases, "<tmp>");
  add(m.bases, "-tmp-", encoded);
  add([m.tmp], "<tmpdir>/", undefined, "/?");
  add([m.tmp], "-tmpdir", encoded);
  add([m.home], "$HOME");
  add([m.home], "-HOME", encoded);
  for (const s of tmpSegments(m)) rules.push([new RegExp(escapeRe(s), "g"), "<tmpid>"]);
  for (const h of m.hosts) rules.push([new RegExp(escapeRe(h), "gi"), "<host>"]);
  for (const t of m.terms) rules.push([new RegExp(escapeRe(t), "gi"), "<redacted>"]);
  if (m.user.length > 2) rules.push([new RegExp(`\\b${escapeRe(m.user)}\\b`, "gi"), "<user>"]);
  rules.push([/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/g, "<email>"]);
  rules.push([/\bsk-[A-Za-z0-9_-]{16,}/g, "<secret>"]);
  rules.push([/\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+/g, "<secret>"]);
  rules.push([/\b(Bearer|bearer)\s+[A-Za-z0-9._~+/=-]{8,}/g, "$1 <secret>"]);
  // Names of the user's own MCP servers and plugins, wherever they appear in text:
  // MCP tool names, plugin MCP servers, and Codex config keys quoted in warnings.
  rules.push([/\bmcp__[A-Za-z0-9_-]+/g, "mcp__<redacted>"]);
  rules.push([/\bplugin:[A-Za-z0-9_.-]+(:[A-Za-z0-9_.-]+)?/g, "plugin:<redacted>"]);
  rules.push([/\bmcp_servers\.[A-Za-z0-9_-]+/g, "mcp_servers.<redacted>"]);
  return rules;
}

// Long base64/hex runs (keys, signatures, hashes). Ids such as UUIDs or
// "rs_<hex>" message ids are shorter or broken up by "-" / "_", so they survive.
const OPAQUE: Rule = [
  /[A-Za-z0-9+/=]{64,}/g,
  (m) => (/[0-9]/.test(m) && /[A-Za-z]/.test(m) && !m.includes("//") ? "<opaque>" : m),
];

const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/** Codex's warnings quote MCP servers' config keys; its MCP client's errors name them. */
const SERVER_IN_TEXT = [
  /\bmcp_servers\.([A-Za-z0-9_-]+)/g,
  /\bOAuth tokens for server ([A-Za-z0-9_-]+)/g,
];

/**
 * The names in any spelling (`foo_bar`, `fooBar`, `FOO-BAR`, `foo bar`), not
 * inside a longer word, though a camelCase key may go on (`fooBarExecutionDurationMs`).
 */
function spellings(names: Set<string>): RegExp | null {
  const anyCase = (part: string) =>
    part.replace(/[A-Za-z]/g, (c) => `[${c.toLowerCase()}${c.toUpperCase()}]`);
  const words = [...names].map((name) =>
    name
      .split(/[^A-Za-z0-9]+|(?<=[a-z0-9])(?=[A-Z])/)
      .filter(Boolean)
      .map(anyCase)
      .join("[\\s_.-]?"),
  );
  words.sort((a, b) => b.length - a.length);
  return words.length
    ? new RegExp(`(?<![A-Za-z0-9])(?:${words.join("|")})(?![a-z0-9])`, "g")
    : null;
}

export class Sanitizer {
  readonly #machine: Machine;
  readonly #rules: Rule[];
  /** Session ID -> `session-NN`, filled from every input before anything is written. */
  readonly #sessions = new Map<string, string>();
  /** Names of the user's MCP servers that Codex shows, likewise; `#names` matches them. */
  readonly #servers = new Set<string>();
  #names: RegExp | null | undefined;
  /** Every rule `#scrubString` applies, in order: set once every input is collected. */
  #all: Rule[] = [];

  constructor(machine: Machine) {
    this.#machine = machine;
    this.#rules = rules(machine);
  }

  /** Session IDs and MCP server names, to redact wherever they occur. */
  collect(v: Json): void {
    // A placeholder such as `<redacted>` is no name.
    const server = (name: unknown) =>
      typeof name === "string" && /^[A-Za-z0-9][\w.-]*$/.test(name) && this.#servers.add(name);
    if (typeof v === "string") {
      for (const re of SERVER_IN_TEXT) for (const m of v.matchAll(re)) server(m[1]);
      return;
    }
    if (Array.isArray(v)) return v.forEach((x) => this.collect(x));
    if (!v || typeof v !== "object") return;
    // A Codex MCP tool call: title `mcp.<server>.<tool>`, raw input {server, tool, arguments}.
    const input = v.rawInput;
    if (v._meta?.is_mcp_tool_call === true || typeof input?.tool === "string") {
      server(input?.server);
      if (typeof v.title === "string") server(/^mcp\.([^.]+)\./.exec(v.title)?.[1]);
    }
    for (const [k, x] of Object.entries(v)) {
      if (k === "sessionId" && typeof x === "string" && UUID.test(x) && !this.#sessions.has(x)) {
        this.#sessions.set(x, `session-${String(this.#sessions.size + 1).padStart(2, "0")}`);
      }
      this.collect(x);
    }
  }

  /** The lines of one recording, redacted, and each text streamed over several frames folded. */
  lines(lines: readonly string[]): string[] {
    if (this.#names === undefined) {
      this.#names = spellings(this.#servers);
      const sessions = [...this.#sessions].map(([id, alias]): Rule => [
        RegExp(escapeRe(id), "g"),
        alias,
      ]);
      const names: Rule[] = this.#names ? [[this.#names, "<redacted>"]] : [];
      this.#all = [...sessions, ...names, ...this.#rules, OPAQUE];
    }
    const frames = lines.map((line) => this.#scrub(structural(JSON.parse(line))));
    for (const pieces of streams(frames)) {
      const texts = this.#all.reduce(
        replacePieces,
        pieces.map(({ at, key }) => at[key]),
      );
      pieces.forEach(({ at, key }, i) => (at[key] = texts[i]));
    }
    return frames.map((frame) => JSON.stringify(frame));
  }

  /** What must not be left in the output: the machine, the session IDs, the generic patterns. */
  leftovers(): Matcher {
    const m = this.#machine;
    const list: [string, string][] = [
      ["home directory", m.home],
      ["home directory", encoded(m.home)],
      ["temporary path", "/private/var"],
      ["temporary path", "-var-folders-"],
      ...tmpSegments(m).map((s) => ["temporary directory", s] as [string, string]),
      ...m.hosts.map((h) => ["host name", h] as [string, string]),
      ...m.terms.map((t, i) => [`local term ${i + 1}`, t] as [string, string]),
      ...[...this.#sessions.keys()].map((id) => ["session ID", id] as [string, string]),
    ];
    if (m.user.length > 2) list.push(["user name", m.user]);
    const names = this.#names;
    const servers: Matcher = (text) => (names ? hits("MCP server name", names, text) : []);
    return either(terms(list), patterns, servers);
  }

  #scrubString(s: string): string {
    return this.#all.reduce(replace, s);
  }

  /** Recursive scrub, keys too (`codex/<server>ExecutionDurationMs`); images pass untouched. */
  #scrub(v: Json): Json {
    if (typeof v === "string") return this.#scrubString(v);
    if (Array.isArray(v)) return v.map((x) => this.#scrub(x));
    if (!v || typeof v !== "object") return v;
    const out: Json = {};
    for (const [k, x] of Object.entries(v)) {
      const image = typeof v.mimeType === "string" && v.mimeType.startsWith("image/");
      const key = this.#scrubString(k);
      if (k === "data" && typeof x === "string" && image) out[key] = x;
      else if (k === "signature" && typeof x === "string") out[key] = "<signature>";
      // Sent to JetBrains AIR clients: the local path of each skill or plugin.
      else if (k === "skillPath" && typeof x === "string") out[key] = "<redacted skill path>";
      else if (/mcp_servers$/i.test(k) && Array.isArray(x))
        out[key] = x.map((e) => this.#server(e));
      else out[key] = this.#scrub(x);
    }
    return out;
  }

  #server(e: Json): Json {
    if (typeof e === "string") return "<mcp server>";
    if (e && typeof e === "object" && "name" in e)
      return { ...this.#scrub(e), name: "<mcp server>" };
    return this.#scrub(e);
  }
}

// ---- structural rules -------------------------------------------------------

/** User-installed skills and commands reveal personal tooling: keep the count, drop the names. */
function redactCommands(cmds: Json[]): Json[] {
  let n = 0;
  return cmds.map((c) => {
    const d = typeof c.description === "string" ? c.description : "";
    const name = String(c.name);
    const userSourced =
      /\((user|project|claude\.ai sync)\)\s*$/.test(d) ||
      d === "<redacted user skill>" ||
      name.startsWith("$") ||
      name.includes(":");
    if (!userSourced) return c;
    n++;
    const alias = name.startsWith("$") ? `$user-skill-${n}` : `user-skill-${n}`;
    // An input hint says what the skill does as much as its description.
    const input =
      typeof c.input?.hint === "string" ? { input: { ...c.input, hint: "<redacted>" } } : {};
    return { ...c, name: alias, description: "<redacted user skill>", ...input };
  });
}

/** Keep public gpt-* models only; the rest of a private gateway catalog is dropped. */
const keepModel = (id: unknown) =>
  typeof id === "string" && /^(gpt-|claude|opus|sonnet|haiku|default)/i.test(id);

function redactModels(obj: Json) {
  if (Array.isArray(obj?.availableModels)) {
    obj.availableModels = obj.availableModels.filter((m: Json) => keepModel(m.modelId));
  }
}

function redactConfigOptions(opts: Json[]) {
  for (const o of opts ?? []) {
    if (o?.category === "model" && Array.isArray(o.options)) {
      o.options = o.options.filter((x: Json) => keepModel(x.value) || Array.isArray(x.options));
    }
  }
}

// Labels this code writes; a second run over its own output keeps them.
const ROLE_LABELS = new Set([
  "adapter (node)",
  "claude auth status",
  "claude CLI",
  "codex wrapper (node)",
  "codex app-server",
  "user-configured tool or MCP server",
]);

/** Process labels keep only what identifies the role; anything else is a user-configured tool. */
function procLabel(label: string): string {
  if (ROLE_LABELS.has(label)) return label;
  if (/^node index\.js/.test(label)) return "adapter (node)";
  if (/^claude auth status/.test(label)) return "claude auth status";
  if (/^claude\b/.test(label)) return "claude CLI";
  if (/^node codex(\.js)? app-server/.test(label)) return "codex wrapper (node)";
  if (/^codex app-server/.test(label)) return "codex app-server";
  const m = label.match(/^(sleep|sh|zsh|bash|git)\b/);
  if (m?.[1]) return m[1];
  return "user-configured tool or MCP server";
}

/** Every process entry ({pid, label}) in a note, whatever key holds it (procs, procsAtExit, tree, victim, ...). */
function relabel(v: Json): Json {
  if (Array.isArray(v)) return v.map(relabel);
  if (!v || typeof v !== "object") return v;
  const out: Json = {};
  for (const [k, x] of Object.entries(v)) {
    out[k] = k === "label" && typeof x === "string" && "pid" in v ? procLabel(x) : relabel(x);
  }
  return out;
}

function structural(f: Json): Json {
  if (f.dir === "note" && f.note) f.note = relabel(f.note);
  const m = f.msg;
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

/**
 * Redacts every `.jsonl` under `inDir` into the same relative path under
 * `outDir`, then scans what it wrote. Returns the output files and what the
 * scan found in them.
 */
export function sanitizeTree(
  inDir: string,
  outDir: string,
  machine: Machine,
): { files: string[]; findings: Finding[] } {
  const files = expand(["."], inDir).filter((f) => f.endsWith(".jsonl"));
  const read = (file: string) =>
    readFileSync(join(inDir, file), "utf8").split("\n").filter(Boolean);
  const sanitizer = new Sanitizer(machine);
  for (const file of files) read(file).forEach((l) => sanitizer.collect(JSON.parse(l)));
  for (const file of files) {
    const out = sanitizer.lines(read(file));
    mkdirSync(dirname(join(outDir, file)), { recursive: true });
    writeFileSync(join(outDir, file), out.join("\n") + "\n");
  }
  return { files, findings: scan(files, outDir, sanitizer.leftovers()) };
}
