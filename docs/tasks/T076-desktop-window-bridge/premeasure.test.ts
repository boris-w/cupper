// T076 pre-measurement (design 5.7.1's judging load, for T078): a windowBridge
// in front of a MockHost, the preload's port on a Node MessageChannel, and a
// consumer that cuts frames, parses JSON and acknowledges at once. One Node
// thread runs all of it, the mock host's encoding and the consumer included,
// so the delay is an upper bound for the bridge alone. No native extension:
// its socketpair forwarding (8 KB buffers) is not in these numbers.
import { monitorEventLoopDelay } from "node:perf_hooks";

import { FrameDecoder, FRAME_JSON } from "@cupper/host-client";
import { MockHost, type MockConnection } from "@cupper/host-client/mock";


import { fixtures, settle, wired } from "../../../apps/desktop/src/main/bridge/testing";

const SECONDS = 10;
const text = (bytes: number) => "x".repeat(bytes);
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

test("the judging load through the bridge", async () => {
  const host = new MockHost(fixtures);
  const { shell } = wired(host, { schedule: (task, ms) => void setTimeout(task, ms) });
  const { port } = await shell.open();
  const conn = host.connections.at(-1) as MockConnection;
  const decoder = new FrameDecoder();
  let [bytes, frames, events] = [0, 0, 0];
  port.onData((chunk) => {
    decoder.push(new Uint8Array(chunk));
    for (let frame = decoder.next(); frame; frame = decoder.next()) {
      frames += 1;
      if (frame.type === FRAME_JSON) {
        const message = JSON.parse(new TextDecoder().decode(frame.payload)) as { params?: { events?: unknown[] } };
        events += message.params?.events?.length ?? 0;
      }
    }
    bytes += chunk.byteLength;
    port.ack(chunk.byteLength);
  });

  const delay = monitorEventLoopDelay({ resolution: 1 });
  delay.enable();
  const stream = (n: number, size: number) => ({
    jsonrpc: "2.0",
    method: "stream/events",
    params: { stream: `s${n}`, events: [{ seq: 1, body: { type: "item.delta", data: { text: text(size) } } }] },
  });
  const timers = [
    setInterval(() => conn.send(stream(0, 1000)), 200),
    ...[1, 2, 3, 4].map((n) => setInterval(() => conn.send(stream(n, 2000)), 40)),
  ];
  const started = performance.now();
  await sleep(3000);
  // A 5 MB blob, as `blob/get` sends it: 82 chunks in 64,000-byte frames.
  const blobStart = performance.now();
  const blobFrom = bytes;
  const chunk = new Uint8Array(63_954);
  for (let offset = 0; offset < 5 * 1024 * 1024; offset += chunk.length) {
    conn.sendBlob({ hash: "ab".repeat(32), offset, last: false, bytes: chunk });
  }
  while (bytes - blobFrom < 5 * 1024 * 1024) {
    await settle();
  }
  const blobMs = performance.now() - blobStart;
  await sleep(3000 - blobMs);
  // A resync of 5,000 events, 100 to a frame.
  const resyncStart = performance.now();
  const before = events;
  for (let i = 0; i < 50; i++) {
    const batch = Array.from({ length: 100 }, (_, j) => ({ seq: i * 100 + j, body: { type: "item.delta", data: { text: text(200) } } }));
    conn.send({ jsonrpc: "2.0", method: "stream/events", params: { stream: "s9", events: batch } });
  }
  while (events - before < 5000) {
    await settle();
  }
  const resyncMs = performance.now() - resyncStart;
  await sleep(SECONDS * 1000 - (performance.now() - started));
  timers.forEach(clearInterval);
  await settle();
  delay.disable();

  const ms = (ns: number) => (ns / 1e6).toFixed(2);
  console.log(
    JSON.stringify({
      seconds: SECONDS,
      frames,
      megabytes: (bytes / 1e6).toFixed(1),
      blobMs: blobMs.toFixed(1),
      resyncMs: resyncMs.toFixed(1),
      loopDelayMs: { p50: ms(delay.percentile(50)), p99: ms(delay.percentile(99)), max: ms(delay.max) },
    }),
  );
  expect(events).toBeGreaterThan(5000);
  port.close();
});
