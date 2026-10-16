// SP-1 scenario scripts. Each scenario writes one JSONL recording per adapter.
// Safety rules enforced here (see brief): prompts are only sent in the default
// or a stricter permission mode, on the cheapest model, inside a temp repo.

import { readdirSync, statSync, readFileSync, existsSync, mkdirSync, realpathSync } from "node:fs";
import { join } from "node:path";
import { homedir } from "node:os";
import { deflateSync } from "node:zlib";
import { AcpConn, CLIENT_CAPABILITIES, CLIENT_INFO, DEFAULT_POLICY, procTree } from "./acp-client.ts";
import type { AdapterSpec, Frame, Json, Policy, RequestResult } from "./acp-client.ts";

export type Family = "claude" | "codex";

export interface Ctx {
  adapter: string;
  family: Family;
  base: string;
  repo: string;
  rawDir: string;
  state: Json;
  saveState(): void;
  spec(extraEnv?: Record<string, string>): AdapterSpec;
  header(scenario: string): Json;
}

/** Modes we may prompt in: the default mode or a stricter one. */
const SAFE_MODES: Record<Family, string[]> = {
  claude: ["default", "plan"],
  codex: ["read-only", "workspace-write"],
};
const WORK_MODE: Record<Family, string> = { claude: "default", codex: "workspace-write" };
const STRICT_MODE: Record<Family, string> = { claude: "default", codex: "read-only" };

// ---------------------------------------------------------------------------
// Session config tracking

interface SessionView {
  configOptions: Json[];
  modeId?: string;
}

class Tracker {
  sessions = new Map<string, SessionView>();
  private reqs = new Map<Json, Json>();

  attach(c: AcpConn) {
    c.onFrame((f) => this.onFrame(f));
  }

  private view(sid: string): SessionView {
    if (!this.sessions.has(sid)) this.sessions.set(sid, { configOptions: [] });
    return this.sessions.get(sid)!;
  }

  private onFrame(f: Frame) {
    const m = f.msg;
    if (!m) return;
    if (f.dir === "send" && m.method && m.id !== undefined) this.reqs.set(m.id, m);
    if (f.dir === "recv" && m.id !== undefined && m.result && this.reqs.has(m.id)) {
      const req = this.reqs.get(m.id);
      const sid = m.result.sessionId ?? req.params?.sessionId;
      if (sid && Array.isArray(m.result.configOptions)) this.view(sid).configOptions = m.result.configOptions;
      if (sid && m.result.modes?.currentModeId) this.view(sid).modeId = m.result.modes.currentModeId;
    }
    if (f.dir === "recv" && m.method === "session/update") {
      const sid = m.params?.sessionId;
      const u = m.params?.update;
      if (!sid || !u) return;
      if (u.sessionUpdate === "config_option_update" && Array.isArray(u.configOptions)) this.view(sid).configOptions = u.configOptions;
      if (u.sessionUpdate === "current_mode_update") this.view(sid).modeId = u.currentModeId;
    }
  }

  option(sid: string, category: string): Json | undefined {
    return this.view(sid).configOptions.find((o: Json) => o.category === category);
  }

  modeValue(sid: string): string | undefined {
    return this.option(sid, "mode")?.currentValue ?? this.view(sid).modeId;
  }
}

function flatOptions(opt: Json): Json[] {
  return (opt?.options ?? []).flatMap((o: Json) => (Array.isArray(o.options) ? o.options : [o]));
}

const CHEAP_MODEL: Record<Family, (o: Json) => boolean> = {
  claude: (o) => /haiku/i.test(`${o.value} ${o.name}`),
  // gpt-5.4-mini is listed but rejected for ChatGPT-account backends; luna is the
  // small model codex-acp itself uses for titles.
  codex: (o) => o.value === "gpt-5.6-luna",
};
const LOW_EFFORT = ["none", "minimal", "low"];

// ---------------------------------------------------------------------------
// Helpers

export function open(ctx: Ctx, scenario: string, extraEnv?: Record<string, string>, t0?: number) {
  const c = new AcpConn(
    ctx.spec(extraEnv),
    join(ctx.rawDir, `${scenario}.jsonl`),
    join(ctx.rawDir, `${scenario}.stderr.log`),
    ctx.header(scenario),
    t0,
  );
  const tracker = new Tracker();
  tracker.attach(c);
  return { c, tracker };
}

async function initialize(c: AcpConn): Promise<RequestResult> {
  return c.request(
    "initialize",
    { protocolVersion: 1, clientCapabilities: CLIENT_CAPABILITIES, clientInfo: CLIENT_INFO },
    60_000,
  );
}

async function boot(c: AcpConn) {
  c.start();
  const init = await initialize(c);
  c.note("initialize-latency", { elapsedMs: init.elapsedMs, ok: init.ok });
  if (!init.ok) throw new Error(`initialize failed: ${JSON.stringify(init.error)}`);
  return init;
}

async function newSession(ctx: Ctx, c: AcpConn): Promise<{ sid: string; res: RequestResult }> {
  const res = await c.request("session/new", { cwd: ctx.repo, mcpServers: [] }, 120_000);
  c.note("session-new-latency", { elapsedMs: res.elapsedMs, ok: res.ok });
  if (!res.ok) throw new Error(`session/new failed: ${JSON.stringify(res.error)}`);
  return { sid: res.result.sessionId, res };
}

async function setOption(c: AcpConn, sid: string, configId: string, value: Json) {
  const r = await c.request("session/set_config_option", { sessionId: sid, configId, value }, 60_000);
  c.note("set-config", { configId, value, ok: r.ok, elapsedMs: r.elapsedMs });
  return r;
}

/** Cheapest model, lowest effort, and the requested (safe) mode. */
async function configure(ctx: Ctx, c: AcpConn, tr: Tracker, sid: string, mode: string) {
  const model = tr.option(sid, "model");
  const cheap = flatOptions(model).find(CHEAP_MODEL[ctx.family]);
  if (!model || !cheap) throw new Error("cheap model not offered; refusing to prompt");
  if (model.currentValue !== cheap.value) {
    const r = await setOption(c, sid, model.id, cheap.value);
    if (!r.ok) throw new Error("could not select cheap model");
  }
  const effort = tr.option(sid, "thought_level");
  if (effort) {
    const values = flatOptions(effort).map((o: Json) => o.value);
    const low = LOW_EFFORT.find((v) => values.includes(v));
    if (low && effort.currentValue !== low) await setOption(c, sid, effort.id, low);
  }
  const modeOpt = tr.option(sid, "mode");
  if (!modeOpt) throw new Error("no mode option");
  if (modeOpt.currentValue !== mode) {
    const r = await setOption(c, sid, modeOpt.id, mode);
    if (!r.ok) throw new Error(`could not set mode ${mode}`);
  }
}

function assertSafe(ctx: Ctx, c: AcpConn, tr: Tracker, sid: string, allowUnknownModel = false) {
  const mode = tr.modeValue(sid);
  const model = tr.option(sid, "model");
  const cheapOk = model ? CHEAP_MODEL[ctx.family]({ value: model.currentValue, name: model.currentValue }) : false;
  const safe = mode !== undefined && SAFE_MODES[ctx.family].includes(mode) && (cheapOk || allowUnknownModel);
  c.note("safety-check", { mode, model: model?.currentValue, safe });
  if (!safe) throw new Error(`refusing to prompt: mode=${mode} model=${model?.currentValue}`);
}

function text(t: string) {
  return [{ type: "text", text: t }];
}

async function prompt(
  ctx: Ctx,
  c: AcpConn,
  tr: Tracker,
  sid: string,
  blocks: Json[],
  label: string,
  timeoutMs = 180_000,
): Promise<RequestResult> {
  assertSafe(ctx, c, tr, sid);
  c.note("prompt", { label });
  const r = await c.request("session/prompt", { sessionId: sid, prompt: blocks }, timeoutMs);
  c.note("prompt-result", { label, ok: r.ok, stopReason: r.result?.stopReason, elapsedMs: r.elapsedMs, timedOut: r.timedOut ?? false });
  return r;
}

function promptAsync(ctx: Ctx, c: AcpConn, tr: Tracker, sid: string, blocks: Json[], label: string, timeoutMs = 180_000) {
  assertSafe(ctx, c, tr, sid);
  c.note("prompt", { label, async: true });
  const { id, done } = c.requestAsync("session/prompt", { sessionId: sid, prompt: blocks }, timeoutMs);
  return {
    id,
    done: done.then((r) => {
      c.note("prompt-result", { label, ok: r.ok, stopReason: r.result?.stopReason, elapsedMs: r.elapsedMs, timedOut: r.timedOut ?? false });
      return r;
    }),
  };
}

function isUpdate(f: Frame, sid: string): boolean {
  return f.dir === "recv" && f.msg?.method === "session/update" && f.msg?.params?.sessionId === sid;
}

function isSleepTool(f: Frame, sid: string): boolean {
  if (!isUpdate(f, sid)) return false;
  const u = f.msg.params.update;
  return (u.sessionUpdate === "tool_call" || u.sessionUpdate === "tool_call_update") && JSON.stringify(u).includes("sleep");
}

/** Wait until the `sleep` command is actually executing (after any approval). */
async function waitSleepRunning(ctx: Ctx, c: AcpConn, sid: string) {
  const first = await c.waitFor((f) => isSleepTool(f, sid), 90_000);
  if (!first) return false;
  // An approval, if one is needed, follows the tool call within a few seconds.
  const isAsk = (f: Frame) => f.dir === "recv" && f.msg?.method === "session/request_permission";
  if (ctx.family === "claude" && !c.frames.some((f) => f.seq > first.seq && isAsk(f))) await c.waitFor(isAsk, 5000);
  await c.settle(1500);
  return true;
}

/** Resolve when nothing arrived from the adapter for `quietMs`, capped at `maxMs`. */
async function waitQuiet(c: AcpConn, quietMs = 8000, maxMs = 120_000) {
  const start = c.now();
  while (c.now() - start < maxMs) {
    const f = await c.waitFor((fr) => fr.dir === "recv", quietMs);
    if (!f) return true;
  }
  return false;
}

function findCliChild(ctx: Ctx, rootPid: number) {
  const { procs } = procTree(rootPid);
  const match = procs.filter((p) =>
    ctx.family === "claude" ? /^claude\b/.test(p.label) : /\bcodex\b/.test(p.label) && p.label.includes("app-server"),
  );
  // the deepest match is the native process (wrappers spawn it)
  return match.length ? match[match.length - 1] : undefined;
}

/** Read-only check of the CLI's own history storage for a session id. */
export function storageCheck(ctx: Ctx, sid: string, configDir?: string) {
  const found: Json[] = [];
  if (ctx.family === "claude") {
    const projects = join(configDir ?? join(homedir(), ".claude"), "projects");
    if (existsSync(projects)) {
      for (const d of readdirSync(projects)) {
        const f = join(projects, d, `${sid}.jsonl`);
        if (existsSync(f)) {
          const body = readFileSync(f, "utf8");
          const lines = body.split("\n").filter(Boolean);
          const types = [...new Set(lines.map((l) => { try { return JSON.parse(l).type; } catch { return "?"; } }))];
          found.push({ where: "claude-projects", bytes: statSync(f).size, lines: lines.length, types });
        }
      }
    }
  } else {
    // Rollouts live in sessions/YYYY/MM/DD/; only yesterday..tomorrow can hold a session made now.
    const root = join(configDir ?? join(homedir(), ".codex"), "sessions");
    for (const offset of [-1, 0, 1]) {
      const d = new Date(Date.now() + offset * 86_400_000);
      const dir = join(root, String(d.getFullYear()), String(d.getMonth() + 1).padStart(2, "0"), String(d.getDate()).padStart(2, "0"));
      if (!existsSync(dir)) continue;
      for (const name of readdirSync(dir).filter((n) => n.includes(sid) && n.endsWith(".jsonl"))) {
        const p = join(dir, name);
        const lines = readFileSync(p, "utf8").split("\n").filter(Boolean);
        found.push({ where: "codex-sessions", bytes: statSync(p).size, lines: lines.length });
      }
    }
    const index = join(configDir ?? join(homedir(), ".codex"), "session_index.jsonl");
    if (existsSync(index) && readFileSync(index, "utf8").includes(sid)) found.push({ where: "codex-session-index" });
  }
  return { sessionId: sid, found };
}

/** 8x8 solid red PNG, generated so no binary fixture is needed. */
export function redPng(): string {
  const w = 8;
  const h = 8;
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf: Buffer) => {
    let c = 0xffffffff;
    for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type: string, data: Buffer) => {
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(type), data]);
    const c = Buffer.alloc(4);
    c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(w, 0);
  ihdr.writeUInt32BE(h, 4);
  ihdr[8] = 8;
  ihdr[9] = 2;
  const rows: Buffer[] = [];
  for (let y = 0; y < h; y++) {
    const row = Buffer.alloc(1 + w * 3);
    for (let x = 0; x < w; x++) row.set([255, 0, 0], 1 + x * 3);
    rows.push(row);
  }
  const png = Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(Buffer.concat(rows))),
    chunk("IEND", Buffer.alloc(0)),
  ]);
  return png.toString("base64");
}

function countingPolicy(sequence: Array<"allow_once" | "reject_once">): Policy {
  let n = 0;
  return {
    ...DEFAULT_POLICY,
    permission: () => sequence[Math.min(n++, sequence.length - 1)],
  };
}

async function fresh(ctx: Ctx, scenario: string, mode: string, extraEnv?: Record<string, string>) {
  const { c, tracker } = open(ctx, scenario, extraEnv);
  await boot(c);
  const { sid } = await newSession(ctx, c);
  await configure(ctx, c, tracker, sid, mode);
  return { c, tracker, sid };
}

// ---------------------------------------------------------------------------
// Scenarios

/** 1. initialize capabilities; when config options and commands arrive; empty sessions. */
export async function s01(ctx: Ctx) {
  const name = "s01-init-session";
  const { c, tracker } = open(ctx, name);
  await boot(c);
  c.sample("after-initialize");
  await c.settle(4000);
  c.sample("idle-no-session");
  const { sid } = await newSession(ctx, c);
  await c.settle(8000);
  c.sample("idle-one-session");
  c.note("storage-check", storageCheck(ctx, sid));
  const list = await c.request("session/list", { cwd: ctx.repo }, 60_000);
  c.note("listed-empty-session", { listed: (list.result?.sessions ?? []).some((s: Json) => s.sessionId === sid), elapsedMs: list.elapsedMs });
  // Selecting options without prompting: shows the update shape and timing.
  await configure(ctx, c, tracker, sid, WORK_MODE[ctx.family]);
  await c.settle(2000);
  const close = await c.request("session/close", { sessionId: sid }, 60_000);
  c.note("session-close", { ok: close.ok, elapsedMs: close.elapsedMs });
  await c.settle(1000);
  c.note("storage-check-after-close", storageCheck(ctx, sid));
  await c.stop();
  c.close();

  const second = open(ctx, name);
  await boot(second.c);
  const list2 = await second.c.request("session/list", { cwd: ctx.repo }, 60_000);
  second.c.note("listed-after-restart", { listed: (list2.result?.sessions ?? []).some((s: Json) => s.sessionId === sid) });
  await second.c.stop();
  second.c.close();
  ctx.state.s01 = { sessionId: sid };
  ctx.saveState();
}

/** 2. A normal turn: streaming, tools, edit diff, plan, usage. */
export async function s02(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s02-normal-turn", WORK_MODE[ctx.family]);
  c.sample("idle");
  void c.waitFor((f) => isUpdate(f, sid) && f.msg.params.update.sessionUpdate === "tool_call", 120_000).then((f) => {
    if (f && !c.exited) c.sample("running");
  });
  await prompt(
    ctx, c, tracker, sid,
    text(
      "Please do these two steps in order and track them with your todo/plan tool: " +
        "(1) create a file named hello.txt containing exactly the line: hello " +
        "(2) in app.js rename the function greet to sayHello, including its export. " +
        "Keep your final reply to one short sentence.",
    ),
    "normal-turn",
  );
  await waitQuiet(c, 5000, 30_000);
  ctx.state.s02 = { sessionId: sid };
  ctx.saveState();
  await c.stop();
  c.close();
}

/** 2b. Plan updates: ask explicitly for the task/plan tool (s02 left it to the model). */
export async function s02b(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s02b-plan", WORK_MODE[ctx.family]);
  await prompt(
    ctx, c, tracker, sid,
    text(
      ctx.family === "claude"
        ? "Use the TaskCreate tool to create two tasks named 'step one' and 'step two', then use TaskUpdate to mark both completed. Do nothing else."
        : "Use your update_plan tool to record a two-step plan ('step one', 'step two'), then call update_plan again marking both steps completed. Do nothing else.",
    ),
    "plan",
  );
  await waitQuiet(c, 4000, 20_000);
  await c.stop();
  c.close();
}

/** 2c. Claude only: s02b again with the CLI's task tools switched on by environment variable. */
export async function s02c(ctx: Ctx) {
  if (ctx.family !== "claude") return;
  const { c, tracker, sid } = await fresh(ctx, "s02c-plan-tasks-enabled", WORK_MODE[ctx.family], { CLAUDE_CODE_ENABLE_TASKS: "1" });
  c.note("env", { CLAUDE_CODE_ENABLE_TASKS: "1" });
  await prompt(
    ctx, c, tracker, sid,
    text("Use the TaskCreate tool to create two tasks named 'step one' and 'step two', then use TaskUpdate to mark both completed. Do nothing else."),
    "plan",
  );
  await waitQuiet(c, 4000, 20_000);
  await c.stop();
  c.close();
}

/** 2d. Claude only: one turn at a higher effort, to see whether thinking is streamed at all (none at "low"). */
export async function s02d(ctx: Ctx) {
  if (ctx.family !== "claude") return;
  const { c, tracker, sid } = await fresh(ctx, "s02d-thinking", WORK_MODE[ctx.family]);
  const effort = tracker.option(sid, "thought_level");
  await setOption(c, sid, effort.id, "high");
  await prompt(ctx, c, tracker, sid, text("Think it through step by step, then reply with only the result: what is 17 * 23 - 19?"), "thinking");
  await waitQuiet(c, 4000, 20_000);
  await c.stop();
  c.close();
}

/** 3. Command execution: output and exit code. */
export async function s03(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s03-exec", WORK_MODE[ctx.family]);
  await prompt(
    ctx, c, tracker, sid,
    text(
      "Run these two shell commands, one tool call each, and nothing else: " +
        "first `ls`, then `sh -c 'echo out; echo err 1>&2; exit 3'`. " +
        "Then reply with one line giving both exit codes.",
    ),
    "exec",
  );
  await waitQuiet(c, 4000, 20_000);
  await c.stop();
  c.close();
}

/** 4. Approvals: allow once, then reject once (Codex #556). */
export async function s04(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s04-approval", STRICT_MODE[ctx.family]);
  c.policy = countingPolicy(["allow_once", "reject_once"]);
  await prompt(
    ctx, c, tracker, sid,
    text(
      "Create a file a.txt containing: a. After that, create a file b.txt containing: b. " +
        "Use one separate tool call per file, in that order. If an action is rejected, " +
        "do not retry it; just tell me which file was not created.",
    ),
    "allow-then-reject",
  );
  await waitQuiet(c, 5000, 30_000);
  await c.stop();
  c.close();
}

/** 4b. Rejecting a command approval (s04 rejected a file edit). */
export async function s04b(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s04b-reject-command", STRICT_MODE[ctx.family]);
  c.policy = countingPolicy(["reject_once"]);
  await prompt(
    ctx, c, tracker, sid,
    // Mentioning rejection up front made the model skip the command, so the prompt is neutral.
    text("Create a file named d.txt by running exactly this shell command: `printf d > d.txt`. Then tell me whether it worked."),
    "reject-command",
  );
  await waitQuiet(c, 5000, 30_000);
  await c.stop();
  c.close();
}

/** 5. Structured questions: AskUserQuestion / request_user_input / plan confirmation. */
export async function s05(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s05-questions", WORK_MODE[ctx.family]);
  if (ctx.family === "claude") {
    await prompt(
      ctx, c, tracker, sid,
      text(
        "Use the AskUserQuestion tool to ask me one question, 'Pick a color', with the options red and blue. " +
          "After I answer, reply with only the chosen color. Do not use any other tool.",
      ),
      "ask-user-question",
    );
    await waitQuiet(c, 4000, 20_000);
    await setOption(c, sid, tracker.option(sid, "mode").id, "plan");
    await prompt(
      ctx, c, tracker, sid,
      text(
        "I want a file named plan.txt containing the word ok. Write a one-line plan and then ask me " +
          "to approve it with ExitPlanMode. After approval, create the file.",
      ),
      "plan-confirmation",
    );
  } else {
    const collab = tracker.option(sid, "collaboration_mode") ??
      tracker.sessions.get(sid)!.configOptions.find((o: Json) => /collab/i.test(o.id));
    c.note("collaboration-option", { id: collab?.id, values: flatOptions(collab).map((o: Json) => o.value) });
    const planValue = flatOptions(collab).find((o: Json) => /plan/i.test(o.value))?.value;
    await setOption(c, sid, collab.id, planValue);
    await prompt(
      ctx, c, tracker, sid,
      text(
        "Before planning, ask me one multiple-choice question with your request_user_input tool: " +
          "'Pick a color' with the options red and blue. Then propose a one-step plan to create " +
          "color.txt containing the chosen color.",
      ),
      "request-user-input-and-plan",
    );
  }
  await waitQuiet(c, 8000, 120_000);
  await c.stop();
  c.close();
}

/** 6. Queueing a second prompt and steering (Claude #1267). */
export async function s06(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s06-queue-steer", WORK_MODE[ctx.family]);

  // A: a second session/prompt while the first turn runs.
  const p1 = promptAsync(ctx, c, tracker, sid, text("Run the shell command `sleep 10`, then reply with exactly: first-done"), "queue-first");
  await waitSleepRunning(ctx, c, sid);
  c.sample("running");
  const p2 = promptAsync(ctx, c, tracker, sid, text("Reply with exactly: second-done"), "queue-second");
  const [r1, r2] = await Promise.all([p1.done, p2.done]);
  c.note("queue-order", { firstEndedAt: r1.t, secondEndedAt: r2.t, first: r1.result?.stopReason ?? r1.error, second: r2.result?.stopReason ?? r2.error });
  await waitQuiet(c, 4000, 30_000);

  // B: steer while the tool call is still being produced (first tool_call frame).
  const p3 = promptAsync(ctx, c, tracker, sid, text("Run the shell command `sleep 10`, then reply with exactly: third-done"), "steer-early");
  await c.waitFor((f) => isUpdate(f, sid) && f.msg.params.update.sessionUpdate === "tool_call", 90_000);
  const s1 = await c.request("_session/steering", { sessionId: sid, prompt: text("Also append the word STEERED to your final reply.") }, 60_000);
  c.note("steer-early-result", { ok: s1.ok, result: s1.result, error: s1.error, elapsedMs: s1.elapsedMs });
  await p3.done;
  await waitQuiet(c, 5000, 60_000);

  // C: steer while the command executes.
  const p4 = promptAsync(ctx, c, tracker, sid, text("Run the shell command `sleep 10`, then reply with exactly: fourth-done"), "steer-running");
  await waitSleepRunning(ctx, c, sid);
  const s2 = await c.request("_session/steering", { sessionId: sid, prompt: text("Also append the word STEERED2 to your final reply.") }, 60_000);
  c.note("steer-running-result", { ok: s2.ok, result: s2.result, error: s2.error, elapsedMs: s2.elapsedMs });
  await p4.done;
  await waitQuiet(c, 5000, 60_000);

  // D: steer while idle, opting into the host-owned fallback.
  assertSafe(ctx, c, tracker, sid);
  const s3 = await c.request(
    "_session/steering",
    { sessionId: sid, prompt: text("Reply with exactly: idle-steer"), _meta: { steering: { idleBehavior: "promptRequired" } } },
    60_000,
  );
  c.note("steer-idle-optin-result", { ok: s3.ok, result: s3.result, error: s3.error });
  await waitQuiet(c, 8000, 60_000);

  // E: steer while idle without the opt-in (only if D did not already start a turn).
  if (s3.result?.outcome === "promptRequired") {
    assertSafe(ctx, c, tracker, sid);
    const s4 = await c.request("_session/steering", { sessionId: sid, prompt: text("Reply with exactly: idle-steer-2") }, 60_000);
    c.note("steer-idle-default-result", { ok: s4.ok, result: s4.result, error: s4.error });
    await waitQuiet(c, 8000, 60_000);
  }
  await c.stop();
  c.close();
}

function ticks(n: number) {
  return `for i in $(seq 1 ${n}); do echo tick $i; sleep 1; done`;
}

/** 7. Cancel timing; cancel with a pending approval; CLI process killed mid-turn (Codex #600). */
export async function s07(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s07-cancel", WORK_MODE[ctx.family]);

  // A: cancel while the command runs.
  // Claude Code refuses a long foreground `sleep N` and backgrounds it, so the
  // long-running command is a loop of one-second sleeps (it also streams output).
  const pa = promptAsync(ctx, c, tracker, sid, text(`Run this shell command in the foreground and wait for it: \`${ticks(30)}\`. Then reply with exactly: done`), "cancel-running");
  await waitSleepRunning(ctx, c, sid);
  c.sample("running");
  const tA = c.now();
  c.notify("session/cancel", { sessionId: sid });
  const ra = await pa.done;
  c.note("cancel-latency", { case: "running", ms: ra.t - tA, stopReason: ra.result?.stopReason });
  await c.settle(3000);
  c.sample("after-cancel");
  await waitQuiet(c, 4000, 30_000);

  // B: cancel while an approval is pending.
  if (ctx.family === "codex") await setOption(c, sid, tracker.option(sid, "mode").id, "read-only");
  c.policy = { ...DEFAULT_POLICY, permission: () => ({ hold: true }) };
  const pb = promptAsync(
    ctx, c, tracker, sid,
    text("Create a file named c.txt containing: c"),
    "cancel-pending-approval",
  );
  const held = await c.waitFor((f) => f.dir === "recv" && f.msg?.method === "session/request_permission", 90_000);
  await c.settle(500);
  const tB = c.now();
  c.notify("session/cancel", { sessionId: sid });
  await c.settle(300);
  for (const h of c.held.splice(0)) c.answerPermission(h.id, h.params, "cancelled");
  const rb = await pb.done;
  c.note("cancel-latency", { case: "pending-approval", heldSeen: !!held, ms: rb.t - tB, stopReason: rb.result?.stopReason });
  c.policy = DEFAULT_POLICY;
  if (ctx.family === "codex") await setOption(c, sid, tracker.option(sid, "mode").id, "workspace-write");
  await waitQuiet(c, 4000, 30_000);

  // C: the session still works after cancelling.
  await prompt(ctx, c, tracker, sid, text("Reply with exactly: after-cancel"), "after-cancel");

  // D: kill the CLI process (not the adapter) in the middle of a turn.
  const pd = promptAsync(ctx, c, tracker, sid, text(`Run this shell command in the foreground and wait for it: \`${ticks(20)}\`. Then reply with exactly: done`), "cli-killed", 90_000);
  await waitSleepRunning(ctx, c, sid);
  const victim = findCliChild(ctx, c.child.pid!);
  c.note("kill-cli", { victim });
  const tD = c.now();
  if (victim) process.kill(victim.pid, "SIGKILL");
  const rd = await pd.done;
  c.note("after-cli-kill", { ms: rd.t - tD, ok: rd.ok, stopReason: rd.result?.stopReason, error: rd.error, timedOut: rd.timedOut ?? false });
  await c.settle(3000);
  c.sample("after-cli-kill");
  // E: is the session usable afterwards?
  if (!c.exited) {
    const re = await prompt(ctx, c, tracker, sid, text("Reply with exactly: alive"), "after-cli-kill-followup", 120_000);
    c.note("followup-after-kill", { ok: re.ok, stopReason: re.result?.stopReason, error: re.error });
  }
  await c.stop();
  c.close();
}

/** 7b. The CLI process dies while the session is idle: what happens next, and does session/resume recover it? */
export async function s07b(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s07b-cli-crash-recovery", WORK_MODE[ctx.family]);
  await prompt(ctx, c, tracker, sid, text("Reply with exactly: one"), "before-kill");
  await waitQuiet(c, 3000, 15_000);
  const victim = findCliChild(ctx, c.child.pid!);
  c.note("kill-cli", { victim, idle: true });
  if (victim) process.kill(victim.pid, "SIGKILL");
  await c.settle(4000);
  c.sample("after-cli-kill");
  const r2 = await prompt(ctx, c, tracker, sid, text("Reply with exactly: two"), "after-kill", 120_000);
  c.note("after-kill-prompt", { ok: r2.ok, stopReason: r2.result?.stopReason, error: r2.error });
  if (!r2.ok) {
    const rr = await c.request("session/resume", { sessionId: sid, cwd: ctx.repo, mcpServers: [] }, 120_000);
    c.note("resume-after-kill", { ok: rr.ok, error: rr.error, elapsedMs: rr.elapsedMs });
    if (rr.ok) {
      await configure(ctx, c, tracker, sid, WORK_MODE[ctx.family]);
      const r3 = await prompt(ctx, c, tracker, sid, text("Reply with exactly: three"), "after-resume", 120_000);
      c.note("after-resume-prompt", { ok: r3.ok, stopReason: r3.result?.stopReason, error: r3.error });
    }
  }
  await waitQuiet(c, 3000, 15_000);
  await c.stop();
  c.close();
}

/** 8. session/list: only counts and field structure of pre-existing sessions are kept. */
export async function s08(ctx: Ctx) {
  const name = "s08-list";
  const { c } = open(ctx, name);
  // Adapters report the real path (macOS: /private/var/...), not the path we passed.
  const bases = [ctx.base, realpathSync(ctx.base)];
  const mine = (s: Json) => typeof s?.cwd === "string" && bases.some((b) => s.cwd.startsWith(b));
  c.redactor = (f) => {
    const sessions = f.msg?.result?.sessions;
    if (f.dir !== "recv" || !Array.isArray(sessions)) return f;
    return {
      ...f,
      msg: {
        ...f.msg,
        result: {
          ...f.msg.result,
          nextCursor: f.msg.result.nextCursor ? "<cursor>" : f.msg.result.nextCursor,
          sessions: sessions.map((s: Json) => (mine(s) ? s : shape(s))),
        },
      },
    };
  };
  await boot(c);
  let cursor: string | undefined;
  let pages = 0;
  let total = 0;
  let ours = 0;
  const shapes = new Map<string, number>();
  const t = c.now();
  do {
    const r = await c.request("session/list", cursor ? { cursor } : {}, 120_000);
    if (!r.ok) {
      c.note("list-error", { error: r.error });
      break;
    }
    pages++;
    for (const s of r.result.sessions ?? []) {
      total++;
      if (mine(s)) ours++;
      const k = JSON.stringify(shape(s));
      shapes.set(k, (shapes.get(k) ?? 0) + 1);
    }
    cursor = r.result.nextCursor ?? undefined;
  } while (cursor && pages < 100);
  c.note("list-summary", {
    pages,
    total,
    fromThisSpike: ours,
    elapsedMs: c.now() - t,
    shapes: [...shapes.entries()].map(([k, n]) => ({ count: n, shape: JSON.parse(k) })),
  });
  const filtered = await c.request("session/list", { cwd: ctx.repo }, 60_000);
  c.note("list-filtered", { cwd: "as passed to session/new", count: filtered.result?.sessions?.length, nextCursor: !!filtered.result?.nextCursor });
  const real = await c.request("session/list", { cwd: realpathSync(ctx.repo) }, 60_000);
  c.note("list-filtered", { cwd: "realpath", count: real.result?.sessions?.length, nextCursor: !!real.result?.nextCursor });
  await c.stop();
  c.close();
}

/** Field structure of a session entry: key -> type (nested for _meta). */
function shape(v: Json): Json {
  if (Array.isArray(v)) return v.length ? [shape(v[0])] : [];
  if (v && typeof v === "object") return Object.fromEntries(Object.keys(v).sort().map((k) => [k, shape(v[k])]));
  return v === null ? "null" : typeof v;
}

/** 9. session/load replay (completeness, timestamps), /compact (#1297), crash + resume. */
export async function s09(ctx: Ctx) {
  const name = "s09-load-resume";
  const target = ctx.state.s02?.sessionId;
  if (!target) throw new Error("s02 session id missing; run s02 first");
  const t0 = Date.now();
  const loadParams = { sessionId: target, cwd: ctx.repo, mcpServers: [] };

  // Process 1: load (replay), compact, then crash.
  {
    const { c, tracker } = open(ctx, name, undefined, t0);
    c.note("process", { n: 1, plan: "load, /compact, SIGKILL" });
    await boot(c);
    const r = await c.request("session/load", loadParams, 120_000);
    c.note("load-result", { ok: r.ok, elapsedMs: r.elapsedMs, replayed: c.frames.filter((f) => isUpdate(f, target)).length });
    await c.settle(3000);
    await configure(ctx, c, tracker, target, WORK_MODE[ctx.family]);
    await prompt(ctx, c, tracker, target, text("/compact"), "compact", 240_000);
    await waitQuiet(c, 5000, 60_000);
    await c.crash();
    c.close();
  }
  // Process 2: load again after compaction.
  {
    const { c } = open(ctx, name, undefined, t0);
    c.note("process", { n: 2, plan: "load after compaction" });
    await boot(c);
    const before = c.frames.length;
    const r = await c.request("session/load", loadParams, 120_000);
    c.note("load-result", { ok: r.ok, elapsedMs: r.elapsedMs, replayed: c.frames.slice(before).filter((f) => isUpdate(f, target)).length });
    await c.settle(3000);
    await c.stop();
    c.close();
  }
  // Process 3: resume (no replay) and continue the conversation.
  {
    const { c, tracker } = open(ctx, name, undefined, t0);
    c.note("process", { n: 3, plan: "resume and continue" });
    await boot(c);
    const before = c.frames.length;
    const r = await c.request("session/resume", loadParams, 120_000);
    c.note("resume-result", { ok: r.ok, elapsedMs: r.elapsedMs, updatesDuringResume: c.frames.slice(before).filter((f) => isUpdate(f, target)).length });
    await c.settle(3000);
    await configure(ctx, c, tracker, target, WORK_MODE[ctx.family]);
    await prompt(
      ctx, c, tracker, target,
      text("Without using any tools: what is the name of the first file you created in this conversation? Answer with the file name only."),
      "continue-after-resume",
    );
    await waitQuiet(c, 4000, 20_000);
    await c.stop();
    c.close();
  }
}

/** 10. Not logged in (empty config dir) — never touches the real login. */
export async function s10(ctx: Ctx) {
  const name = "s10-unauthenticated";
  const emptyDir = join(ctx.base, "empty", ctx.adapter);
  mkdirSync(emptyDir, { recursive: true });
  const env = ctx.family === "claude" ? { CLAUDE_CONFIG_DIR: emptyDir } : { CODEX_HOME: emptyDir };
  const { c, tracker } = open(ctx, name, env);
  c.note("env", { emptyConfigDir: true, variable: Object.keys(env)[0] });
  c.start();
  const init = await initialize(c);
  c.note("initialize-latency", { elapsedMs: init.elapsedMs, ok: init.ok });
  await c.settle(5000);
  const ns = await c.request("session/new", { cwd: ctx.repo, mcpServers: [] }, 120_000);
  c.note("session-new", { ok: ns.ok, error: ns.error, elapsedMs: ns.elapsedMs });
  if (ns.ok) {
    const sid = ns.result.sessionId;
    try {
      await configure(ctx, c, tracker, sid, WORK_MODE[ctx.family]);
    } catch (e) {
      c.note("configure-failed", { message: String(e) });
    }
    const mode = tracker.modeValue(sid);
    if (mode && SAFE_MODES[ctx.family].includes(mode)) {
      assertSafe(ctx, c, tracker, sid, true);
      const r = await c.request("session/prompt", { sessionId: sid, prompt: text("Reply with exactly: hi") }, 120_000);
      c.note("prompt-result", { ok: r.ok, stopReason: r.result?.stopReason, error: r.error, elapsedMs: r.elapsedMs, timedOut: r.timedOut ?? false });
    }
    c.note("storage-check", storageCheck(ctx, sid, emptyDir));
  }
  await c.settle(2000);
  await c.stop();
  c.close();
}

/** 11. A tiny image in the prompt. */
export async function s11(ctx: Ctx) {
  const { c, tracker, sid } = await fresh(ctx, "s11-image", WORK_MODE[ctx.family]);
  await prompt(
    ctx, c, tracker, sid,
    [
      { type: "text", text: "What is the main color of the attached image? Answer with one word. Do not use tools." },
      { type: "image", mimeType: "image/png", data: redPng() },
    ],
    "image",
  );
  await waitQuiet(c, 4000, 20_000);
  await c.stop();
  c.close();
}

/** 12. Process tree and memory: idle adapter, then 1..3 idle sessions. Running samples come from s06/s07. */
export async function s12(ctx: Ctx) {
  const { c } = open(ctx, "s12-resources");
  const tSpawn = Date.now();
  await boot(c);
  c.note("time-to-initialized", { ms: Date.now() - tSpawn });
  await c.settle(5000);
  c.sample("idle-0-sessions");
  const sids: string[] = [];
  for (let i = 1; i <= 3; i++) {
    const { sid } = await newSession(ctx, c);
    sids.push(sid);
    await c.settle(6000);
    c.sample(`idle-${i}-sessions`);
  }
  for (const sid of sids) await c.request("session/close", { sessionId: sid }, 60_000);
  await c.settle(4000);
  c.sample("after-close-all");
  await c.stop();
  c.close();
  ctx.state.s12 = { sessionIds: sids };
  ctx.saveState();
}

export const SCENARIOS: Record<string, (ctx: Ctx) => Promise<void>> = {
  s01, s02, s02b, s02c, s02d, s03, s04, s04b, s05, s06, s07, s07b, s08, s09, s10, s11, s12,
};
