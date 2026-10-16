// Minimal ACP client used by SP-1 to record JSON-RPC frames from real adapters.
// It speaks raw newline-delimited JSON-RPC over stdio on purpose: every frame is
// recorded exactly as it crossed the wire, including extension methods and
// `_meta` fields that a typed SDK might normalize or drop.

import { spawn, execFileSync } from "node:child_process";
import type { ChildProcess } from "node:child_process";
import { createWriteStream } from "node:fs";
import type { WriteStream } from "node:fs";
import { createInterface } from "node:readline";
import { basename } from "node:path";

export type Json = any;

export interface Frame {
  seq: number;
  t: number;
  dir: "send" | "recv" | "note" | "recv-nonjson";
  msg?: Json;
  note?: string;
  data?: Json;
  line?: string;
}

export interface AdapterSpec {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
}

/** What the client answers to `session/request_permission`. */
export type PermissionDecision =
  | "allow_once"
  | "reject_once"
  | "cancelled"
  | { hold: true };

export interface Policy {
  permission: (params: Json, conn: AcpConn) => PermissionDecision;
  elicitation: (params: Json, conn: AcpConn) => Json;
}

export interface RequestResult {
  ok: boolean;
  result?: Json;
  error?: Json;
  timedOut?: boolean;
  t: number;
  elapsedMs: number;
}

export interface HeldPermission {
  id: Json;
  params: Json;
  t: number;
}

/** Client capabilities declared in `initialize`, shared by every scenario. */
export const CLIENT_CAPABILITIES = {
  fs: { readTextFile: false, writeTextFile: false },
  terminal: false,
  session: {
    configOptions: { boolean: {} },
    notices: {},
    compaction: {},
  },
  plan: {},
  elicitation: { form: {}, url: {} },
  auth: { terminal: true },
  _meta: { terminal_output: true },
};

export const CLIENT_INFO = { name: "cupper-sp1-client", title: "cupper SP-1 probe", version: "0.0.1" };

/** Never answer with a persistent option; only the two one-shot kinds are allowed. */
function pickOption(options: Json[], kind: "allow_once" | "reject_once"): Json | undefined {
  return (options ?? []).find((o: Json) => o?.kind === kind);
}

/** Fill an elicitation form with the first allowed value of every field. */
export function autoFillForm(schema: Json): Json {
  const content: Json = {};
  const props = schema?.properties ?? {};
  for (const [key, prop] of Object.entries<Json>(props)) {
    content[key] = firstValue(prop);
  }
  return content;
}

function firstValue(prop: Json): Json {
  if (Array.isArray(prop?.enum) && prop.enum.length > 0) return prop.enum[0];
  if (Array.isArray(prop?.oneOf) && prop.oneOf.length > 0) return prop.oneOf[0].const;
  if (Array.isArray(prop?.anyOf) && prop.anyOf.length > 0 && prop.anyOf[0].const !== undefined) return prop.anyOf[0].const;
  switch (prop?.type) {
    case "array": {
      const items = prop.items ?? {};
      const v = firstValue(items);
      return v === undefined || v === "" ? [] : [v];
    }
    case "boolean":
      return true;
    case "number":
    case "integer":
      return prop.minimum ?? 1;
    case "string":
      return prop.default ?? "ok";
    default:
      return undefined;
  }
}

export const DEFAULT_POLICY: Policy = {
  permission: () => "allow_once",
  elicitation: (params) => {
    if (params?.mode === "url") return { action: "decline" };
    return { action: "accept", content: autoFillForm(params?.requestedSchema) };
  },
};

/** Connections whose adapter process is still running (for cleanup after a failure). */
export const LIVE = new Set<AcpConn>();

export class AcpConn {
  readonly spec: AdapterSpec;
  child!: ChildProcess;
  private out: WriteStream;
  private stderrOut: WriteStream;
  private t0 = Date.now();
  private seq = 0;
  private nextId = 1;
  private pending = new Map<number, { resolve: (r: RequestResult) => void; t: number; timer?: NodeJS.Timeout }>();
  private waiters: Array<{ pred: (f: Frame) => boolean; resolve: (f: Frame | null) => void; timer: NodeJS.Timeout }> = [];
  readonly frames: Frame[] = [];
  policy: Policy = DEFAULT_POLICY;
  readonly held: HeldPermission[] = [];
  exited: { code: number | null; signal: string | null; t: number } | null = null;
  private exitWaiters: Array<() => void> = [];
  /** Optional rewrite applied before a frame is stored or written (must not mutate). */
  redactor: ((f: Frame) => Frame) | null = null;
  private listeners: Array<(f: Frame) => void> = [];

  constructor(spec: AdapterSpec, recordingPath: string, stderrPath: string, header: Json, t0?: number) {
    this.spec = spec;
    if (t0 !== undefined) this.t0 = t0;
    this.out = createWriteStream(recordingPath, { flags: "a" });
    this.stderrOut = createWriteStream(stderrPath, { flags: "a" });
    this.note("header", header);
  }

  now(): number {
    return Date.now() - this.t0;
  }

  onFrame(fn: (f: Frame) => void) {
    this.listeners.push(fn);
  }

  private write(original: Frame) {
    for (const l of this.listeners) l(original);
    const frame = this.redactor ? this.redactor(original) : original;
    this.frames.push(frame);
    this.out.write(JSON.stringify(frame) + "\n");
    for (const w of [...this.waiters]) {
      if (w.pred(original)) {
        clearTimeout(w.timer);
        this.waiters.splice(this.waiters.indexOf(w), 1);
        w.resolve(original);
      }
    }
  }

  note(note: string, data?: Json) {
    this.write({ seq: ++this.seq, t: this.now(), dir: "note", note, data });
  }

  start() {
    const startedAt = this.now();
    this.child = spawn(this.spec.command, this.spec.args, {
      env: this.spec.env,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.note("spawn", { pid: this.child.pid, adapter: this.spec.name, t: startedAt });
    LIVE.add(this);
    const rl = createInterface({ input: this.child.stdout! });
    rl.on("line", (line) => this.onLine(line));
    this.child.stderr!.on("data", (d: Buffer) => {
      this.stderrOut.write(`[${this.now()}] ` + d.toString());
    });
    this.child.on("exit", (code, signal) => {
      this.exited = { code, signal, t: this.now() };
      LIVE.delete(this);
      this.note("exit", { code, signal });
      for (const [, p] of this.pending) {
        if (p.timer) clearTimeout(p.timer);
        p.resolve({ ok: false, error: { local: "adapter exited" }, t: this.now(), elapsedMs: this.now() - p.t });
      }
      this.pending.clear();
      for (const w of this.exitWaiters.splice(0)) w();
    });
    this.child.stdin!.on("error", () => {});
  }

  private onLine(line: string) {
    if (!line.trim()) return;
    let msg: Json;
    try {
      msg = JSON.parse(line);
    } catch {
      this.write({ seq: ++this.seq, t: this.now(), dir: "recv-nonjson", line });
      return;
    }
    this.write({ seq: ++this.seq, t: this.now(), dir: "recv", msg });
    if (msg.method !== undefined && msg.id !== undefined) {
      this.onAgentRequest(msg);
    } else if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined)) {
      const p = this.pending.get(msg.id);
      if (p) {
        this.pending.delete(msg.id);
        if (p.timer) clearTimeout(p.timer);
        p.resolve({
          ok: msg.error === undefined,
          result: msg.result,
          error: msg.error,
          t: this.now(),
          elapsedMs: this.now() - p.t,
        });
      }
    }
  }

  private send(msg: Json) {
    this.write({ seq: ++this.seq, t: this.now(), dir: "send", msg });
    if (this.child.stdin!.writable) this.child.stdin!.write(JSON.stringify(msg) + "\n");
  }

  request(method: string, params: Json, timeoutMs = 120_000): Promise<RequestResult> {
    const id = this.nextId++;
    const t = this.now();
    return new Promise((resolve) => {
      const entry: { resolve: (r: RequestResult) => void; t: number; timer?: NodeJS.Timeout } = { resolve, t };
      entry.timer = setTimeout(() => {
        this.pending.delete(id);
        this.note("request-timeout", { id, method, timeoutMs });
        resolve({ ok: false, timedOut: true, t: this.now(), elapsedMs: this.now() - t });
      }, timeoutMs);
      this.pending.set(id, entry);
      this.send({ jsonrpc: "2.0", id, method, params });
    });
  }

  /** Send a request without awaiting; returns the id and a promise for the result. */
  requestAsync(method: string, params: Json, timeoutMs = 120_000): { id: number; done: Promise<RequestResult> } {
    const id = this.nextId;
    return { id, done: this.request(method, params, timeoutMs) };
  }

  notify(method: string, params: Json) {
    this.send({ jsonrpc: "2.0", method, params });
  }

  respond(id: Json, result: Json) {
    this.send({ jsonrpc: "2.0", id, result });
  }

  respondError(id: Json, code: number, message: string) {
    this.send({ jsonrpc: "2.0", id, error: { code, message } });
  }

  private onAgentRequest(msg: Json) {
    const { id, method, params } = msg;
    if (method === "session/request_permission") {
      const decision = this.policy.permission(params, this);
      if (typeof decision === "object" && decision.hold) {
        this.held.push({ id, params, t: this.now() });
        this.note("permission-held", { id });
        return;
      }
      this.answerPermission(id, params, decision as Exclude<PermissionDecision, { hold: true }>);
      return;
    }
    if (method === "elicitation/create") {
      this.respond(id, this.policy.elicitation(params, this));
      return;
    }
    this.respondError(id, -32601, `Method not supported by SP-1 client: ${method}`);
  }

  answerPermission(id: Json, params: Json, decision: "allow_once" | "reject_once" | "cancelled") {
    if (decision === "cancelled") {
      this.respond(id, { outcome: { outcome: "cancelled" } });
      return;
    }
    let option = pickOption(params?.options, decision);
    if (!option && decision === "allow_once") {
      this.note("no-allow-once-option", { id });
      option = pickOption(params?.options, "reject_once");
    }
    if (!option) {
      this.respond(id, { outcome: { outcome: "cancelled" } });
      return;
    }
    this.respond(id, { outcome: { outcome: "selected", optionId: option.optionId } });
  }

  /** Resolve with the first future frame matching `pred`, or null on timeout. */
  waitFor(pred: (f: Frame) => boolean, timeoutMs: number): Promise<Frame | null> {
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.waiters.splice(this.waiters.findIndex((w) => w.timer === timer), 1);
        resolve(null);
      }, timeoutMs);
      this.waiters.push({ pred, resolve, timer });
    });
  }

  /** Collect frames for a fixed quiet period (used to see what arrives unprompted). */
  async settle(ms: number) {
    await new Promise((r) => setTimeout(r, ms));
  }

  waitExit(timeoutMs: number): Promise<boolean> {
    if (this.exited) return Promise.resolve(true);
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(false), timeoutMs);
      this.exitWaiters.push(() => {
        clearTimeout(timer);
        resolve(true);
      });
    });
  }

  /** Graceful stop: close stdin, wait, then escalate. Records leftover descendants. */
  async stop(graceMs = 8000) {
    if (this.exited) return;
    const before = procTree(this.child.pid!);
    this.note("stop-requested", { mode: "stdin-close", tree: before.procs.length });
    this.child.stdin!.end();
    const ok = await this.waitExit(graceMs);
    if (!ok) {
      this.note("stop-escalate", { signal: "SIGTERM" });
      this.child.kill("SIGTERM");
      await this.waitExit(5000);
    }
    await this.reapOrphans(before.procs.map((p) => p.pid).filter((pid) => pid !== this.child.pid));
  }

  /** Hard kill of the adapter only (crash simulation). */
  async crash() {
    const before = procTree(this.child.pid!);
    this.note("crash", { signal: "SIGKILL", tree: before });
    this.child.kill("SIGKILL");
    await this.waitExit(5000);
    await new Promise((r) => setTimeout(r, 3000));
    await this.reapOrphans(before.procs.map((p) => p.pid).filter((pid) => pid !== this.child.pid));
  }

  /** Report descendants that outlived the adapter, then terminate them. */
  private async reapOrphans(pids: number[]) {
    // Give descendants up to 5 s to exit on their own before reporting them.
    const start = this.now();
    let alive = pids.filter(isAlive);
    const atExit = describe(alive);
    while (alive.length > 0 && this.now() - start < 5000) {
      await new Promise((r) => setTimeout(r, 250));
      alive = alive.filter(isAlive);
    }
    this.note("orphans-after-exit", { aliveAtExit: atExit.length, aliveAfter5s: alive.length, procsAtExit: atExit, waitedMs: this.now() - start });
    for (const pid of alive) {
      try {
        process.kill(pid, "SIGTERM");
      } catch {}
    }
    if (alive.length > 0) {
      await new Promise((r) => setTimeout(r, 2000));
      for (const pid of alive.filter(isAlive)) {
        try {
          process.kill(pid, "SIGKILL");
        } catch {}
      }
    }
  }

  sample(label: string) {
    const tree = procTree(this.child.pid!);
    this.note("proc-sample", { label, ...tree });
    return tree;
  }

  close() {
    this.out.end();
    this.stderrOut.end();
  }
}

export interface ProcInfo {
  pid: number;
  ppid: number;
  rssKB: number;
  label: string;
}

function psAll(): ProcInfo[] {
  const out = execFileSync("ps", ["-axo", "pid=,ppid=,rss=,args="], { encoding: "utf8", maxBuffer: 16 * 1024 * 1024 });
  const procs: ProcInfo[] = [];
  for (const line of out.split("\n")) {
    const m = line.trim().match(/^(\d+)\s+(\d+)\s+(\d+)\s+(.*)$/);
    if (!m) continue;
    procs.push({ pid: +m[1], ppid: +m[2], rssKB: +m[3], label: labelOf(m[4]) });
  }
  return procs;
}

/** A short, path-free label: basenames of the program and of its first two args. */
function labelOf(args: string): string {
  const parts = args.split(/\s+/).slice(0, 3);
  return parts.map((p) => (p.includes("/") ? basename(p) : p)).join(" ");
}

export function procTree(rootPid: number): { procs: ProcInfo[]; totalRssKB: number } {
  const all = psAll();
  const byParent = new Map<number, ProcInfo[]>();
  for (const p of all) {
    if (!byParent.has(p.ppid)) byParent.set(p.ppid, []);
    byParent.get(p.ppid)!.push(p);
  }
  const root = all.find((p) => p.pid === rootPid);
  const procs: ProcInfo[] = [];
  const stack = root ? [root] : [];
  while (stack.length) {
    const p = stack.pop()!;
    procs.push(p);
    stack.push(...(byParent.get(p.pid) ?? []));
  }
  return { procs, totalRssKB: procs.reduce((s, p) => s + p.rssKB, 0) };
}

function isAlive(pid: number): boolean {
  try {
    process.kill(pid, 0);
    return true;
  } catch {
    return false;
  }
}

function describe(pids: number[]): ProcInfo[] {
  if (pids.length === 0) return [];
  const all = psAll();
  return all.filter((p) => pids.includes(p.pid));
}
