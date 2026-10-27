// T200: Runtime.evaluate("kill()") over the inspector, 400 times, as
// Playwright's electronApp.evaluate reaches the main process. Run with
// Electron's Node: ELECTRON_RUN_AS_NODE=1 <Electron> parent.mjs (DEFER=1 to defer).
import { spawn } from "node:child_process";
const child = spawn(process.execPath, ["--inspect=127.0.0.1:0", "child.cjs"], { stdio: ["ignore", "inherit", "pipe"], env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } });
const url = await new Promise((done) => child.stderr.on("data", (d) => { const m = /ws:\/\/\S+/.exec(String(d)); if (m) done(m[0]); }));
const ws = new WebSocket(url);
await new Promise((done) => (ws.onopen = done));
let id = 0; const pending = new Map();
ws.onmessage = (m) => { const msg = JSON.parse(m.data); pending.get(msg.id)?.(msg); };
const call = (expression) => new Promise((done) => { pending.set(++id, done); ws.send(JSON.stringify({ id, method: "Runtime.evaluate", params: { expression } })); });
for (let i = 0; i < 400; i++) { await call("kill()"); await new Promise((r) => setTimeout(r, 15)); }
console.log((await call("report()")).result.result.value, process.version);
child.kill();
process.exit(0);
