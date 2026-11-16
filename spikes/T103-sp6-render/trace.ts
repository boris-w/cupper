// T159: Chromium traces of SP-6's scenarios, only with SP6_TRACE=1. The
// window's DevTools protocol session records (`Tracing`, as the Performance
// panel does) the renderer's timeline and V8's samples, as JSON:
// `contentTracing` in the main process writes Perfetto's protobuf, which
// `summarize.mjs` does not read. The renderer marks each stretch to sum up as
// `sp6:<label>:start` and `sp6:<label>:end` (`console.timeStamp`). Each
// recording goes to `results/<name>.trace.json`; `summarize.mjs` sums it up
// after the cases. Tracing slows the app down: the numbers of a traced run
// are not the scenario's.

import { createWriteStream, mkdirSync } from "node:fs";
import { join } from "node:path";

import type { Cupper } from "../../tools/e2e/src/fixture";
import { RESULTS } from "./measure";

type CDPSession = Awaited<ReturnType<ReturnType<Cupper["window"]["context"]>["newCDPSession"]>>;

export const TRACING = process.env.SP6_TRACE === "1";

const CATEGORIES = [
  "devtools.timeline",
  "disabled-by-default-devtools.timeline",
  "disabled-by-default-devtools.timeline.frame",
  "toplevel",
  "v8",
  "v8.execute",
  "disabled-by-default-v8.cpu_profiler",
];

let session: CDPSession | undefined;

/** Starts recording; nothing without SP6_TRACE=1. */
export async function startTrace(cupper: Cupper): Promise<void> {
  if (!TRACING) return;
  session = await cupper.window.context().newCDPSession(cupper.window);
  await session.send("Tracing.start", {
    traceConfig: { includedCategories: CATEGORIES, excludedCategories: ["*"], recordMode: "recordUntilFull" },
    transferMode: "ReturnAsStream",
    streamFormat: "json",
  });
}

/** Stops recording, into `results/<name>.trace.json`. */
export async function stopTrace(_cupper: Cupper, name: string): Promise<void> {
  if (!TRACING || !session) return;
  const cdp = session;
  session = undefined;
  const complete = new Promise<{ stream?: string }>((done) => cdp.once("Tracing.tracingComplete", done));
  await cdp.send("Tracing.end");
  const { stream } = await complete;
  if (!stream) throw new Error("the trace came without a stream");
  mkdirSync(RESULTS, { recursive: true });
  const out = createWriteStream(join(RESULTS, `${name}.trace.json`));
  for (;;) {
    const { data, eof, base64Encoded } = await cdp.send("IO.read", { handle: stream, size: 1 << 20 });
    out.write(base64Encoded ? Buffer.from(data, "base64") : data);
    if (eof) break;
  }
  await cdp.send("IO.close", { handle: stream });
  await new Promise((done) => out.end(done));
  await cdp.detach();
}

/** Marks the start or the end of stretch `label` in the renderer. */
export async function mark(cupper: Cupper, label: string, edge: "start" | "end"): Promise<void> {
  if (!TRACING) return;
  await cupper.window.evaluate((name) => console.timeStamp(name), `sp6:${label}:${edge}`);
}
