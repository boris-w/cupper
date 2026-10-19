// Shared pieces of the T003 scripts: how the adapter is started, the safety
// rules every prompt passes through, and process-tree sampling with process
// group and session IDs. The ACP client itself is SP-1's `AcpConn`, reused as is.

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, join } from "node:path";
import { AcpConn, CLIENT_CAPABILITIES, CLIENT_INFO } from "../SP-1-adapter-behavior/acp-client.ts";
import type { AdapterSpec, Frame, Json, RequestResult } from "../SP-1-adapter-behavior/acp-client.ts";

export function arg(name: string, fallback?: string): string {
  const i = process.argv.indexOf(`--${name}`);
  if (i >= 0 && process.argv[i + 1]) return process.argv[i + 1];
  if (fallback !== undefined) return fallback;
  throw new Error(`missing --${name}`);
}

export function pkgVersion(dir: string): string {
  try {
    return JSON.parse(readFileSync(join(dir, "package.json"), "utf8")).version;
  } catch {
    return "unknown";
  }
}

// Only a small allowlist of the parent environment reaches the adapter (same as
// SP-1), so variables of the calling tool and any secrets stay out.
const KEEP_ENV = ["HOME", "PATH", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "USER", "LOGNAME"];

/**
 * The adapter is started the way the host will start it (design 5.6.1):
 * as the leader of its own session and process group (`POSIX_SPAWN_SETSID`).
 * Node's `spawn` in SP-1's client has no setsid option, so a one-line Perl
 * wrapper calls setsid(2) and then execs node; the pid stays the same.
 */
export function adapterSpec(adapterDir: string, name: string): AdapterSpec {
  const entry = join(adapterDir, "node_modules/@agentclientprotocol/claude-agent-acp/dist/index.js");
  if (!existsSync(entry)) throw new Error(`adapter not installed: ${entry}`);
  const env: Record<string, string> = {};
  for (const k of KEEP_ENV) if (process.env[k]) env[k] = process.env[k]!;
  env.CLAUDE_CODE_EXECUTABLE = process.env.T003_CLAUDE_BIN ?? execFileSync("/usr/bin/which", ["claude"], { encoding: "utf8" }).trim();
  return {
    name,
    command: "/usr/bin/perl",
    args: ["-MPOSIX", "-e", "defined(POSIX::setsid()) or die qq(setsid: $!); exec { $ARGV[0] } @ARGV or die qq(exec: $!)", process.execPath, entry],
    env,
  };
}

export function versionsOf(adapterDir: string, claudeBin: string): Json {
  const nm = join(adapterDir, "node_modules");
  let cli = "unknown";
  try {
    cli = execFileSync(claudeBin, ["--version"], { encoding: "utf8", timeout: 20_000 }).trim();
  } catch {}
  return {
    adapter: `@agentclientprotocol/claude-agent-acp ${pkgVersion(join(nm, "@agentclientprotocol/claude-agent-acp"))}`,
    acpSdk: pkgVersion(join(nm, "@agentclientprotocol/sdk")),
    agentSdk: pkgVersion(join(nm, "@anthropic-ai/claude-agent-sdk")),
    cli,
    node: process.version,
  };
}

/** Throwaway git repo under the run's temp dir, never inside a real project. */
export function makeRepo(path: string) {
  if (existsSync(join(path, ".git"))) return;
  mkdirSync(path, { recursive: true });
  writeFileSync(join(path, "README.md"), "# t003 sandbox\n\nThrowaway repository for adapter tests.\n");
  const git = (...a: string[]) => execFileSync("git", a, { cwd: path, stdio: "ignore" });
  git("init", "-q", "-b", "main");
  git("config", "user.name", "cupper-t003");
  git("config", "user.email", "t003@example.invalid");
  git("config", "commit.gpgsign", "false");
  git("add", ".");
  git("commit", "-q", "-m", "init");
}

// ---------------------------------------------------------------------------
// Session config tracking and the safety gate

export class ConfigTracker {
  private options = new Map<string, Json[]>();
  private modes = new Map<string, string>();
  private reqs = new Map<Json, Json>();

  constructor(c: AcpConn) {
    c.onFrame((f) => this.onFrame(f));
  }

  private onFrame(f: Frame) {
    const m = f.msg;
    if (!m) return;
    if (f.dir === "send" && m.method && m.id !== undefined) this.reqs.set(m.id, m);
    if (f.dir === "recv" && m.id !== undefined && m.result && this.reqs.has(m.id)) {
      const sid = m.result.sessionId ?? this.reqs.get(m.id).params?.sessionId;
      if (sid && Array.isArray(m.result.configOptions)) this.options.set(sid, m.result.configOptions);
      if (sid && m.result.modes?.currentModeId) this.modes.set(sid, m.result.modes.currentModeId);
    }
    if (f.dir === "recv" && m.method === "session/update") {
      const sid = m.params?.sessionId;
      const u = m.params?.update;
      if (u?.sessionUpdate === "config_option_update" && Array.isArray(u.configOptions)) this.options.set(sid, u.configOptions);
      if (u?.sessionUpdate === "current_mode_update") this.modes.set(sid, u.currentModeId);
    }
  }

  option(sid: string, category: string): Json | undefined {
    return (this.options.get(sid) ?? []).find((o: Json) => o.category === category);
  }

  mode(sid: string): string | undefined {
    return this.option(sid, "mode")?.currentValue ?? this.modes.get(sid);
  }
}

const flat = (opt: Json): Json[] => (opt?.options ?? []).flatMap((o: Json) => (Array.isArray(o.options) ? o.options : [o]));
const isHaiku = (v: unknown) => /haiku/i.test(String(v ?? ""));
const LOW_EFFORT = ["none", "minimal", "low"];

async function setOption(c: AcpConn, sid: string, configId: string, value: Json) {
  const r = await c.request("session/set_config_option", { sessionId: sid, configId, value }, 60_000);
  c.note("set-config", { sessionId: sid, configId, value, ok: r.ok, elapsedMs: r.elapsedMs });
  return r;
}

/** Cheapest model (haiku), lowest effort, and the permission mode set explicitly to `default`. */
export async function configure(c: AcpConn, tr: ConfigTracker, sid: string) {
  const model = tr.option(sid, "model");
  const cheap = flat(model).find((o: Json) => isHaiku(o.value) || isHaiku(o.name));
  if (!model || !cheap) throw new Error("haiku not offered; refusing to prompt");
  if (model.currentValue !== cheap.value && !(await setOption(c, sid, model.id, cheap.value)).ok) throw new Error("could not select haiku");
  const effort = tr.option(sid, "thought_level");
  if (effort) {
    const values = flat(effort).map((o: Json) => o.value);
    const low = LOW_EFFORT.find((v) => values.includes(v));
    if (low && effort.currentValue !== low) await setOption(c, sid, effort.id, low);
  }
  const mode = tr.option(sid, "mode");
  if (!mode) throw new Error("no mode option");
  // Set even when it is already `default`: the brief asks for an explicit choice.
  if (!(await setOption(c, sid, mode.id, "default")).ok) throw new Error("could not set mode default");
}

/** Throws (and nothing is sent) unless the session is in `default` mode on haiku. */
export function assertSafe(c: AcpConn, tr: ConfigTracker, sid: string) {
  const mode = tr.mode(sid);
  const model = tr.option(sid, "model")?.currentValue;
  const safe = mode === "default" && isHaiku(model);
  c.note("safety-check", { sessionId: sid, mode, model, safe });
  if (!safe) throw new Error(`refusing to prompt: mode=${mode} model=${model}`);
}

export async function boot(c: AcpConn): Promise<RequestResult> {
  c.start();
  const init = await c.request("initialize", { protocolVersion: 1, clientCapabilities: CLIENT_CAPABILITIES, clientInfo: CLIENT_INFO }, 60_000);
  c.note("initialize-latency", { elapsedMs: init.elapsedMs, ok: init.ok });
  if (!init.ok) throw new Error(`initialize failed: ${JSON.stringify(init.error)}`);
  return init;
}

export async function newSession(c: AcpConn, tr: ConfigTracker, cwd: string): Promise<string> {
  const res = await c.request("session/new", { cwd, mcpServers: [] }, 120_000);
  c.note("session-new-latency", { elapsedMs: res.elapsedMs, ok: res.ok });
  if (!res.ok) throw new Error(`session/new failed: ${JSON.stringify(res.error)}`);
  const sid = res.result.sessionId;
  await configure(c, tr, sid);
  return sid;
}

// ---------------------------------------------------------------------------
// Process tree with process group and session IDs (SP-4b leftover item 7)

export interface Proc {
  pid: number;
  ppid: number;
  pgid: number;
  sid: number;
  tpgid: number;
  stat: string;
  rssKB: number;
  label: string;
}

/** Short, path-free label in SP-1's format: basenames of the program and its first two args. */
function labelOf(args: string): string {
  return args.split(/\s+/).slice(0, 3).map((p) => (p.includes("/") ? basename(p) : p)).join(" ");
}

/** The adapter and all its descendants. `ps -o sess` is always 0 on macOS, so getsid(2) comes from Python. */
export function procTree(rootPid: number): Proc[] {
  const out = execFileSync("ps", ["-axo", "pid=,ppid=,pgid=,tpgid=,stat=,rss=,args="], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  const all: Omit<Proc, "sid">[] = [];
  for (const line of out.split("\n")) {
    const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(-?\d+)\s+(\S+)\s+(\d+)\s+(.*)$/);
    if (m) all.push({ pid: +m[1], ppid: +m[2], pgid: +m[3], tpgid: +m[4], stat: m[5], rssKB: +m[6], label: labelOf(m[7]) });
  }
  const tree: Omit<Proc, "sid">[] = [];
  const stack = all.filter((p) => p.pid === rootPid);
  while (stack.length) {
    const p = stack.pop()!;
    tree.push(p);
    stack.push(...all.filter((q) => q.ppid === p.pid));
  }
  if (!tree.length) return [];
  const sids = new Map<number, number>();
  const py = "import os,sys\nfor p in sys.argv[1:]:\n  try: print(p, os.getsid(int(p)))\n  except OSError: print(p, -1)";
  for (const l of execFileSync("/usr/bin/python3", ["-I", "-c", py, ...tree.map((p) => String(p.pid))], { encoding: "utf8" }).split("\n")) {
    const [pid, sid] = l.trim().split(/\s+/).map(Number);
    if (pid) sids.set(pid, sid);
  }
  return tree.map((p) => ({ ...p, sid: sids.get(p.pid) ?? -1 }));
}
