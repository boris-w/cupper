// T159: Chromium traces of SP-6's scenarios, only with SP6_TRACE=1. The main
// process records (`contentTracing`) the renderer's timeline and V8's
// samples; the renderer marks each stretch to sum up as `sp6:<label>:start`
// and `sp6:<label>:end` (`console.timeStamp`). Each recording goes to
// `results/<name>.trace.json`; `summarize.mjs` sums it up after the cases.
// Tracing slows the app down: the numbers of a traced run are not the
// scenario's.

import { mkdirSync } from "node:fs";
import { join } from "node:path";

import type { Cupper } from "../../tools/e2e/src/fixture";
import { RESULTS } from "./measure";

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

/** Starts recording; nothing without SP6_TRACE=1. */
export async function startTrace(cupper: Cupper): Promise<void> {
  if (!TRACING) return;
  await cupper.app.evaluate(
    ({ contentTracing }, included_categories) =>
      contentTracing.startRecording({
        included_categories,
        excluded_categories: ["*"],
        recording_mode: "record-until-full",
        trace_buffer_size_in_kb: 400 * 1024,
      }),
    CATEGORIES,
  );
}

/** Stops recording, into `results/<name>.trace.json`. */
export async function stopTrace(cupper: Cupper, name: string): Promise<void> {
  if (!TRACING) return;
  mkdirSync(RESULTS, { recursive: true });
  const file = join(RESULTS, `${name}.trace.json`);
  await cupper.app.evaluate(({ contentTracing }, file) => contentTracing.stopRecording(file), file);
}

/** Marks the start or the end of stretch `label` in the renderer. */
export async function mark(cupper: Cupper, label: string, edge: "start" | "end"): Promise<void> {
  if (!TRACING) return;
  await cupper.window.evaluate((name) => console.timeStamp(name), `sp6:${label}:${edge}`);
}
