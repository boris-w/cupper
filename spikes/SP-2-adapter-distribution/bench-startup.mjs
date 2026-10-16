#!/usr/bin/env node
// Start-up benchmark for adapter packaging variants. No session is created and
// no model is called.
//
// Usage: node bench-startup.mjs <configs.json> [runs=10] [cwd]
// configs.json: [{ "name": "...", "cmd": ["/path/bin", "arg", ...], "env": { "K": "V" },
//                  "versionArgs": ["--version"], "skipInit": false }, ...]
//
// For every config it measures, over <runs> runs (plus the very first run
// reported separately as "first"):
//   version_ms: spawn of `<cmd> --version` until exit
//   init_ms:    spawn of `<cmd>` until the ACP `initialize` response arrives
// The adapter is then shut down by closing stdin (SIGKILL after 3 s).
// Prints a JSON array with median / p90 / min / max / first per metric.

import { spawn } from "node:child_process";
import { readFileSync } from "node:fs";
import { performance } from "node:perf_hooks";

const [cfgFile, runsArg, cwdArg] = process.argv.slice(2);
const configs = JSON.parse(readFileSync(cfgFile, "utf8"));
const runs = Number(runsArg ?? 10);
const cwd = cwdArg ?? process.cwd();
const baseEnv = { HOME: process.env.HOME, USER: process.env.USER, TMPDIR: process.env.TMPDIR, LANG: "en_US.UTF-8", PATH: "/usr/bin:/bin:/usr/sbin:/sbin" };

function timeVersion(c) {
  return new Promise((resolve, reject) => {
    const t = performance.now();
    const p = spawn(c.cmd[0], [...c.cmd.slice(1), ...(c.versionArgs ?? ["--version"])], { cwd, env: { ...baseEnv, ...c.env }, stdio: ["ignore", "ignore", "ignore"] });
    p.on("exit", (code) => (code === 0 ? resolve(performance.now() - t) : reject(new Error(`${c.name} --version exit ${code}`))));
  });
}

function timeInit(c) {
  return new Promise((resolve, reject) => {
    const t = performance.now();
    const p = spawn(c.cmd[0], c.cmd.slice(1), { cwd, env: { ...baseEnv, ...c.env }, stdio: ["pipe", "pipe", "ignore"] });
    let buf = "";
    let elapsed = null;
    const killer = setTimeout(() => p.kill("SIGKILL"), 30000);
    p.stdout.on("data", (d) => {
      buf += d;
      if (elapsed === null && buf.includes('"id":1') && buf.includes('"result"')) {
        elapsed = performance.now() - t;
        p.stdin.end();
        setTimeout(() => p.kill("SIGKILL"), 3000).unref();
      }
    });
    p.on("exit", () => {
      clearTimeout(killer);
      elapsed === null ? reject(new Error(`${c.name}: no initialize response`)) : resolve(elapsed);
    });
    p.stdin.write(JSON.stringify({ jsonrpc: "2.0", id: 1, method: "initialize", params: { protocolVersion: 1, clientCapabilities: {}, clientInfo: { name: "cupper-sp2-bench", version: "0.0.0" } } }) + "\n");
  });
}

const stats = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const q = (p) => s[Math.min(s.length - 1, Math.floor(p * s.length))];
  return { median: Math.round(q(0.5)), p90: Math.round(q(0.9)), min: Math.round(s[0]), max: Math.round(s[s.length - 1]) };
};

const out = [];
for (const c of configs) {
  const row = { name: c.name };
  for (const [metric, fn] of [["version_ms", timeVersion], ["init_ms", timeInit]]) {
    if (metric === "init_ms" && c.skipInit) continue;
    try {
      const first = await fn(c);
      const xs = [];
      for (let i = 0; i < runs; i++) xs.push(await fn(c));
      row[metric] = { first: Math.round(first), ...stats(xs) };
    } catch (e) {
      row[metric] = { error: e.message };
    }
  }
  out.push(row);
  process.stderr.write(`${c.name}: ${JSON.stringify(row)}\n`);
}
console.log(JSON.stringify(out, null, 1));
