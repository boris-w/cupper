// The window bridge's inbound copy (T077's handoff): every chunk from the
// host goes through a FrameDecoder, which joins it to what is left and
// copies each whole frame out, before the bridge batches the chunk for the
// window. Here the same work on the judging load's bytes, cut as the native
// extension's socketpair hands them over (8 KB reads), with no JSON parse
// (the retransmit table is empty): its time per second of load.
import { encodeBlobChunk, encodeJson, FrameDecoder } from "@cupper/host-client";

const READ = 8192;

/** One second of the judging load on the wire: 105 stream frames, and a 5 MB blob once. */
function wire(blob: boolean): Uint8Array {
  const frames: Uint8Array[] = [];
  const text = (n: number) => "x".repeat(n);
  for (let n = 0; n < 100; n++) frames.push(encodeJson({ method: "stream/events", params: { events: [{ data: text(2000) }] } }));
  for (let n = 0; n < 5; n++) frames.push(encodeJson({ method: "stream/events", params: { events: [{ data: text(1000) }] } }));
  if (blob) {
    for (let offset = 0; offset < 5 * 1024 * 1024; offset += 63_954) {
      frames.push(encodeBlobChunk({ hash: "ab".repeat(32), offset, last: false, bytes: new Uint8Array(63_954) }));
    }
  }
  const all = new Uint8Array(frames.reduce((n, f) => n + f.length, 0));
  frames.reduce((at, f) => (all.set(f, at), at + f.length), 0);
  return all;
}

test("the inbound copy's time", () => {
  for (const blob of [false, true]) {
    const bytes = wire(blob);
    const runs: number[] = [];
    for (let run = 0; run < 20; run++) {
      const decoder = new FrameDecoder();
      const started = performance.now();
      for (let at = 0; at < bytes.length; at += READ) {
        decoder.push(bytes.subarray(at, at + READ));
        while (decoder.next()) {
          // Frames are only cut, as the bridge does with an empty retransmit table.
        }
      }
      runs.push(performance.now() - started);
    }
    runs.sort((a, b) => a - b);
    console.log(JSON.stringify({ blob, megabytes: (bytes.length / 1e6).toFixed(2), msMedian: runs[10]?.toFixed(2), msMax: runs.at(-1)?.toFixed(2) }));
  }
});
