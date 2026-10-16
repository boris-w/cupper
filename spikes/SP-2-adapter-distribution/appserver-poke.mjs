#!/usr/bin/env node
// Send a few raw JSON-RPC requests to `codex app-server` and print the replies,
// to record what a protocol mismatch looks like. No thread or turn is created.
//
// Usage: node appserver-poke.mjs <codex binary> <experimentalApi:true|false> <method> [<method> ...]
// Each extra method is sent after `initialize` with a dummy threadId, so that
// parameter parsing passes and the capability gate is reached.

import { spawn } from "node:child_process";

const [bin, exp, ...methods] = process.argv.slice(2);
const child = spawn(bin, ["app-server"], { stdio: ["pipe", "pipe", "ignore"] });
const replies = new Map();
let buf = "";
child.stdout.on("data", (d) => {
  buf += d;
  let nl;
  while ((nl = buf.indexOf("\n")) >= 0) {
    const msg = JSON.parse(buf.slice(0, nl));
    buf = buf.slice(nl + 1);
    if (msg.id !== undefined && replies.has(msg.id)) replies.get(msg.id)(msg);
  }
});
const call = (id, method, params) =>
  new Promise((resolve) => {
    replies.set(id, resolve);
    child.stdin.write(JSON.stringify({ id, method, params }) + "\n");
  });

await call(0, "initialize", { clientInfo: { name: "cupper-sp2-probe", version: "0.0.0" }, capabilities: { experimentalApi: exp === "true" } });
child.stdin.write(JSON.stringify({ method: "initialized" }) + "\n");
let id = 1;
for (const m of methods) {
  const reply = await call(id++, m, { threadId: "00000000-0000-0000-0000-000000000000" });
  console.log(JSON.stringify({ method: m, reply: reply.error ? { error: reply.error } : { result: "ok" } }));
}
child.kill();
