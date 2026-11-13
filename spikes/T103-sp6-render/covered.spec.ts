// SP-6 scenario 8 (T103; design 5.9.7): the app started with Chromium's own
// handling of a covered window (no switches against it), a session on screen
// and a window of the same size, opaque and on top, put over it by the main
// process. The page must then say `document.visibilityState === "hidden"`;
// if it does not, the environment does not qualify (T078 saw Playwright keep
// pages visible), which is recorded and ends the test. Otherwise the session
// streams for 10 minutes, the cover goes, and the run records the stream
// resets and the time from uncovering to the page's first frame.

import { test } from "../../tools/e2e/src/fixture";
import { untilEvent } from "../../tools/e2e/src/helpers";
import { id, openSession, session } from "../../tools/e2e/src/nfr1";
import { cover } from "../../tools/e2e/src/perf";
import { record } from "./measure";

const MINUTES = 10;
type Back = Window & { __sp6Back?: number };

test("SP-6 scenario 8: ten minutes streaming covered, then uncovered", async ({ cupper }) => {
  test.setTimeout((MINUTES + 10) * 60_000);
  await cupper.launch({ throttled: true });
  await cupper.connected();
  const stream = await session(cupper, "say s8");
  const said = await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
  await openSession(cupper, "say s8");
  await cover(cupper, { stopFrames: false });
  await cupper.window.waitForTimeout(2000);
  const visibility = await cupper.window.evaluate(() => document.visibilityState);
  if (visibility !== "hidden") {
    record("8-covered", { qualified: false, visibility });
    return;
  }
  await cupper.cli.ok(["send", id(stream), `flood_rate 100 ${MINUTES * 60 + 60} 67`]);
  await untilEvent(cupper, stream, (e) => e.type === "item.delta" && e.seq > said.seq);
  const resets = await cupper.window.evaluate(() => window.__cupperGate.resets());
  await cupper.window.waitForTimeout(MINUTES * 60_000);
  // A frame asked for while hidden comes once the page shows again.
  await cupper.window.evaluate(() =>
    requestAnimationFrame((at) => ((window as Back).__sp6Back = performance.timeOrigin + at)),
  );
  const uncovered = await cupper.app.evaluate(({ BrowserWindow }) => {
    BrowserWindow.getAllWindows()
      .filter((w) => !w.isFocusable())
      .forEach((w) => w.close());
    return Date.now();
  });
  const back = await cupper.window.waitForFunction(() => (window as Back).__sp6Back);
  record("8-covered", {
    qualified: true,
    first_frame_ms: Math.round(((await back.jsonValue()) as number) - uncovered),
    resets: (await cupper.window.evaluate(() => window.__cupperGate.resets())) - resets,
    visible_after: await cupper.window.evaluate(() => document.visibilityState),
  });
});
