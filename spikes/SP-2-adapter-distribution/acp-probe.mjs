#!/usr/bin/env node
// Minimal ACP v1 client used by SP-2 to check that an adapter starts, creates a
// session and (optionally) finishes one short prompt turn. No dependencies.
//
// Usage:
//   node acp-probe.mjs --log <file.jsonl> [--cwd <dir>] [--prompt <text>]
//        [--model <regex>] [--mode <regex>] [--effort <regex>] [--second-prompt <text>]
//        [--pause-before-second <ms>] [--timeout <ms>]
//        [--clean-env 1] [--set KEY=VAL ...] [--clobber <path>] -- <command> [args...]
//
// The probe prints one JSON summary line to stdout. Every JSON-RPC frame is
// appended to --log with a monotonic timestamp (ms since spawn).
// Agent->client requests: permission requests are answered "cancelled" (the
// strictest answer); everything else gets "method not found" because the probe
// advertises no fs/terminal capabilities.

import { spawn } from "node:child_process";
import { appendFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

function parseArgs(argv) {
  const sep = argv.indexOf("--");
  if (sep < 0) throw new Error("missing -- <command>");
  const opts = { cwd: process.cwd(), timeout: 120000, set: [] };
  const flags = argv.slice(0, sep);
  for (let i = 0; i < flags.length; i += 2) {
    const key = flags[i].replace(/^--/, "").replace(/-([a-z])/g, (_, c) => c.toUpperCase());
    if (key === "set") opts.set.push(flags[i + 1]);
    else opts[key] = flags[i + 1];
  }
  opts.timeout = Number(opts.timeout);
  if (opts.pauseBeforeSecond) opts.pauseBeforeSecond = Number(opts.pauseBeforeSecond);
  opts.command = argv[sep + 1];
  opts.args = argv.slice(sep + 2);
  return opts;
}

const opts = parseArgs(process.argv.slice(2));
const t0 = performance.now();
const now = () => Math.round(performance.now() - t0);
if (opts.log) writeFileSync(opts.log, "");
const log = (dir, frame) => {
  if (opts.log) appendFileSync(opts.log, JSON.stringify({ t: now(), dir, frame }) + "\n");
};

// --clean-env 1 mimics what launchd gives a GUI app: no shell PATH, no
// developer env vars. --set KEY=VAL (repeatable) adds variables on top.
const env = opts.cleanEnv
  ? {
      HOME: process.env.HOME,
      USER: process.env.USER,
      LOGNAME: process.env.USER,
      TMPDIR: process.env.TMPDIR,
      LANG: "en_US.UTF-8",
      PATH: "/usr/bin:/bin:/usr/sbin:/sbin",
    }
  : { ...process.env };
for (const kv of opts.set) {
  const i = kv.indexOf("=");
  env[kv.slice(0, i)] = kv.slice(i + 1);
}
const child = spawn(opts.command, opts.args, { cwd: opts.cwd, stdio: ["pipe", "pipe", "pipe"], env });
let stderr = "";
child.stderr.on("data", (d) => {
  stderr += d;
  if (opts.log) appendFileSync(opts.log, JSON.stringify({ t: now(), dir: "stderr", text: String(d) }) + "\n");
});

const summary = { command: opts.command, timings: {}, steps: [], updates: {}, agentText: "" };
let exited = null;
child.on("exit", (code, signal) => {
  exited = { code, signal, t: now() };
  for (const p of pending.values()) p.reject(new Error(`agent exited (code=${code}, signal=${signal})`));
  pending.clear();
});

let nextId = 1;
const pending = new Map();
function send(frame) {
  log("out", frame);
  child.stdin.write(JSON.stringify(frame) + "\n");
}
function request(method, params) {
  const id = nextId++;
  send({ jsonrpc: "2.0", id, method, params });
  return new Promise((resolve, reject) => pending.set(id, { resolve, reject, method }));
}

let buf = "";
child.stdout.on("data", (d) => {
  buf += d;
  let nl;
  while ((nl = buf.indexOf("\n")) >= 0) {
    const line = buf.slice(0, nl).trim();
    buf = buf.slice(nl + 1);
    if (!line) continue;
    let msg;
    try {
      msg = JSON.parse(line);
    } catch {
      log("stdout-garbage", line);
      continue;
    }
    log("in", msg);
    handle(msg);
  }
});

function handle(msg) {
  if (msg.id !== undefined && (msg.result !== undefined || msg.error !== undefined) && !msg.method) {
    const p = pending.get(msg.id);
    if (!p) return;
    pending.delete(msg.id);
    if (msg.error) p.reject(Object.assign(new Error(msg.error.message), { rpc: msg.error }));
    else p.resolve(msg.result);
    return;
  }
  if (msg.method === "session/update") {
    const u = msg.params?.update ?? {};
    const kind = u.sessionUpdate ?? "unknown";
    summary.updates[kind] = (summary.updates[kind] ?? 0) + 1;
    if (kind === "agent_message_chunk" && u.content?.type === "text") summary.agentText += u.content.text;
    return;
  }
  if (msg.id !== undefined && msg.method) {
    if (msg.method === "session/request_permission") {
      summary.steps.push({ step: "permission_request_cancelled", t: now() });
      send({ jsonrpc: "2.0", id: msg.id, result: { outcome: { outcome: "cancelled" } } });
    } else {
      summary.steps.push({ step: "unsupported_client_method", method: msg.method, t: now() });
      send({ jsonrpc: "2.0", id: msg.id, error: { code: -32601, message: "Method not found" } });
    }
  }
}

async function step(name, fn) {
  const start = now();
  try {
    const result = await fn();
    summary.timings[name] = now() - start;
    summary.steps.push({ step: name, ok: true, t: now() });
    return result;
  } catch (err) {
    summary.timings[name] = now() - start;
    summary.steps.push({ step: name, ok: false, t: now(), error: err.message, rpc: err.rpc });
    throw err;
  }
}

function pickOption(configOptions, category, pattern) {
  const opt = (configOptions ?? []).find((o) => o.category === category || o.id === category);
  if (!opt) return null;
  const flat = (opt.options ?? []).flatMap((o) => (o.options ? o.options : [o]));
  const hit = flat.find((o) => new RegExp(pattern, "i").test(String(o.value)));
  return { configId: opt.id, value: hit ? hit.value : null };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function run() {
  const init = await step("initialize", () =>
    request("initialize", {
      protocolVersion: 1,
      clientCapabilities: { fs: { readTextFile: false, writeTextFile: false }, terminal: false },
      clientInfo: { name: "cupper-sp2-probe", version: "0.0.0" },
    }),
  );
  summary.agentInfo = init.agentInfo;
  summary.protocolVersion = init.protocolVersion;
  summary.authMethods = (init.authMethods ?? []).map((m) => m.id);

  const sess = await step("session/new", () => request("session/new", { cwd: opts.cwd, mcpServers: [] }));
  const sessionId = sess.sessionId;
  summary.currentMode = sess.modes?.currentModeId;

  // --model / --mode / --effort take a regex matched against the option values
  // the adapter advertises in session/new (configOptions).
  for (const [flag, category] of [["model", "model"], ["mode", "mode"], ["effort", "thought_level"]]) {
    if (!opts[flag]) continue;
    const pick = pickOption(sess.configOptions, category, opts[flag]);
    summary[`${flag}Choice`] = pick && { configId: pick.configId, value: pick.value };
    if (pick?.value) {
      await step(`set_${flag}`, () => request("session/set_config_option", { sessionId, configId: pick.configId, value: pick.value }));
    }
  }

  // --clobber <path>: simulate an upgrade replacing the adapter on disk while it
  // runs. The path is moved to <path>.moved; if it was a file, a stub that throws
  // on load is written in its place.
  if (opts.clobber) {
    const wasFile = statSync(opts.clobber).isFile();
    renameSync(opts.clobber, `${opts.clobber}.moved`);
    if (wasFile) writeFileSync(opts.clobber, 'throw new Error("adapter file was replaced");\n');
    summary.steps.push({ step: "clobbered", path: opts.clobber.split("/").slice(-2).join("/"), wasFile, t: now() });
  }

  if (opts.prompt) {
    const res = await step("prompt",() => request("session/prompt", { sessionId, prompt: [{ type: "text", text: opts.prompt }] }));
    summary.stopReason = res.stopReason;
  }
  if (opts.secondPrompt) {
    if (opts.pauseBeforeSecond) {
      summary.steps.push({ step: "pause", ms: opts.pauseBeforeSecond, t: now() });
      await sleep(opts.pauseBeforeSecond);
    }
    summary.agentText += " | ";
    const res = await step("prompt2", () => request("session/prompt", { sessionId, prompt: [{ type: "text", text: opts.secondPrompt }] }));
    summary.stopReason2 = res.stopReason;
  }
}

const timer = setTimeout(() => {
  summary.timeout = true;
  finish(1);
}, opts.timeout);

function finish(code) {
  clearTimeout(timer);
  summary.exited = exited;
  summary.totalMs = now();
  summary.stderrTail = stderr.slice(-1500);
  process.stdout.write(JSON.stringify(summary) + "\n");
  child.stdin.end();
  child.kill("SIGTERM");
  setTimeout(() => process.exit(code), 300).unref();
}

run().then(
  () => finish(0),
  (err) => {
    summary.error = err.message;
    finish(1);
  },
);
