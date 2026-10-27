// A Unix socket flooded by its own server; `globalThis.kill()` destroys the
// client, as the e2e hook does. Counts RangeErrors out of onStreamRead.
const net = require("node:net");
const sock = `/tmp/t200-${process.pid}.sock`;
let errors = 0, kills = 0, client;
process.on("uncaughtException", (e) => { if (String(e).includes("out of range")) errors++; else console.log(String(e)); });
const connect = () => { client = net.connect(sock); client.on("data", () => {}); client.on("error", () => {}); };
net.createServer((c) => {
  c.on("error", () => {});
  const chunk = Buffer.alloc(600, 97);
  const pump = () => { if (!c.destroyed) { c.write(chunk); setImmediate(pump); } };
  pump();
}).listen(sock, connect);
// DEFER=1 destroys on a turn of its own, as the fixed e2e hook does.
const kill = () => { kills++; client.destroy(); setTimeout(connect, 1); };
globalThis.kill = () => (process.env.DEFER === "1" ? setImmediate(kill) : kill());
globalThis.report = () => JSON.stringify({ kills, errors });
