// Shared by the T102 scripts: how codex-acp is started (as the host will: its
// own session and process group, CODEX_PATH set to the native codex binary),
// the safety gate every prompt passes, and process-tree roles. The ACP client
// is SP-1's `AcpConn`; config tracking and `ps` sampling come from T003.

import { execFileSync } from "node:child_process";
import { existsSync } from "node:fs";
import { join } from "node:path";
import type { AcpConn, AdapterSpec, Json } from "../SP-1-adapter-behavior/acp-client.ts";
import { pkgVersion, procTree } from "../T003-tech-validation/common.ts";
import type { ConfigTracker } from "../T003-tech-validation/common.ts";

const KEEP_ENV = ["HOME", "PATH", "SHELL", "TMPDIR", "LANG", "LC_ALL", "LC_CTYPE", "USER", "LOGNAME"];
const SAFE_MODES = ["read-only", "workspace-write"];
/** The small model codex-acp itself uses for titles (SP-1: the cheapest one the gateway accepts). */
export const MODEL = process.env.T102_MODEL ?? "gpt-5.6-luna";
const modules = (base: string, v: string) => join(base, "adapters", `codex-${v}`, "node_modules");

/** setsid(2) through a one-line Perl wrapper, then exec node (as in T003); the pid stays the same. */
export function codexSpec(base: string, version: string, codexBin: string, name: string): AdapterSpec {
  const entry = join(modules(base, version), "@agentclientprotocol/codex-acp/dist/index.js");
  if (!existsSync(entry)) throw new Error(`adapter not installed: ${entry}`);
  const env: Record<string, string> = { CODEX_PATH: codexBin };
  for (const k of KEEP_ENV) if (process.env[k]) env[k] = process.env[k]!;
  const setsid = "defined(POSIX::setsid()) or die qq(setsid: $!); exec { $ARGV[0] } @ARGV or die qq(exec: $!)";
  return { name, command: "/usr/bin/perl", args: ["-MPOSIX", "-e", setsid, process.execPath, entry], env };
}

/** First line of every recording: versions, date, the capabilities declared, the plan. */
export function header(base: string, version: string, codexBin: string, scenario: string, clientCapabilities: Json, plan?: Json): Json {
  let cli = "unknown";
  try {
    cli = execFileSync(codexBin, ["--version"], { encoding: "utf8", timeout: 20_000 }).trim();
  } catch {}
  const nm = modules(base, version);
  const versions = { adapter: `@agentclientprotocol/codex-acp ${pkgVersion(join(nm, "@agentclientprotocol/codex-acp"))}`, acpSdk: pkgVersion(join(nm, "@agentclientprotocol/sdk")), cli, node: process.version };
  return { scenario, versions, date: new Date().toISOString().slice(0, 10), clientCapabilities, plan };
}

const flat = (opt: Json): Json[] => (opt?.options ?? []).flatMap((o: Json) => (Array.isArray(o.options) ? o.options : [o]));

async function set(c: AcpConn, sid: string, configId: string, value: string) {
  const r = await c.request("session/set_config_option", { sessionId: sid, configId, value }, 60_000);
  c.note("set-config", { sessionId: sid, configId, value, ok: r.ok, elapsedMs: r.elapsedMs });
  if (!r.ok) throw new Error(`could not set ${configId}=${value}`);
}

/** New session on the cheapest model and lowest effort; mode and collaboration mode always set explicitly. */
export async function newSession(c: AcpConn, tr: ConfigTracker, cwd: string, mode = "read-only", collab = "default"): Promise<string> {
  if (!SAFE_MODES.includes(mode)) throw new Error(`refusing mode ${mode}`);
  const res = await c.request("session/new", { cwd, mcpServers: [] }, 120_000);
  c.note("session-new-latency", { elapsedMs: res.elapsedMs, ok: res.ok });
  if (!res.ok) throw new Error(`session/new failed: ${JSON.stringify(res.error)}`);
  const sid = res.result.sessionId;
  const model = tr.option(sid, "model");
  if (!flat(model).some((o) => o.value === MODEL)) throw new Error(`${MODEL} not offered; refusing to prompt`);
  if (model.currentValue !== MODEL) await set(c, sid, model.id, MODEL);
  const effort = tr.option(sid, "thought_level");
  const low = ["none", "minimal", "low"].find((v) => flat(effort).some((o) => o.value === v));
  if (effort && low && effort.currentValue !== low) await set(c, sid, effort.id, low);
  await set(c, sid, "mode", mode);
  await set(c, sid, "collaboration_mode", collab);
  return sid;
}

/** Throws (and nothing is sent) unless mode, model and collaboration mode are the safe ones. */
export function assertSafe(c: AcpConn, tr: ConfigTracker, sid: string, collab = "default") {
  const [mode, model, co] = [tr.mode(sid), tr.option(sid, "model")?.currentValue, tr.option(sid, "collaboration_mode")?.currentValue];
  const safe = SAFE_MODES.includes(mode ?? "") && model === MODEL && co === collab;
  c.note("safety-check", { sessionId: sid, mode, model, collaborationMode: co, safe });
  if (!safe) throw new Error(`refusing to prompt: mode=${mode} model=${model} collaboration=${co}`);
}

/**
 * One sample of the adapter's tree. Per process only its role, ids, and whether
 * its process group and session are the adapter's or the app-server's; labels
 * (program and first arguments) go to `priv`, which is never committed.
 */
export function sampleTree(root: number, label: string, t: number, priv: Json[]): Json {
  const tree = procTree(root);
  priv.push({ t, label, tree });
  const role = (p: { pid: number; label: string }) =>
    p.pid === root ? "adapter (node)" : /^codex app-server/.test(p.label) ? "codex app-server" : (p.label.match(/^(sandbox-exec|sleep|sh|zsh|bash)\b/)?.[1] ?? "user-configured tool or MCP server");
  const ad = tree.find((p) => p.pid === root);
  const app = tree.find((p) => role(p) === "codex app-server");
  const procs = tree.map((p) => ({
    role: role(p),
    ...{ pid: p.pid, ppid: p.ppid, pgid: p.pgid, sid: p.sid, groupLeader: p.pgid === p.pid, sessionLeader: p.sid === p.pid },
    ...{ sameGroupAsAdapter: p.pgid === ad?.pgid, sameSessionAsAdapter: p.sid === ad?.sid, sameGroupAsAppServer: p.pgid === app?.pgid, sameSessionAsAppServer: p.sid === app?.sid },
  }));
  return { t, label, procs };
}
