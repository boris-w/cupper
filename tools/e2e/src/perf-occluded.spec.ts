// PERF-ui-occluded (design 5.9.7 SP-6 scenario 8; T103, T155), recorded only:
// the app started with Chromium's own handling of a covered window, a session
// on screen, then hidden. Under Playwright a page stayed `visible` whatever
// covered it (T078, T103): Playwright emulates focus on each page over CDP,
// which keeps it counted as visible, so that is turned off for this page
// first. Then a window of the same size, opaque and on top, covers the app's,
// and failing that the window is minimized. Not `hidden` either way: the
// environment does not qualify, which the test records and ends on.
// Otherwise the session streams for 10 minutes, the window shows again, and
// the stream resets and the time to the page's first frame are recorded.

import { test } from "./fixture";
import { untilEvent } from "./helpers";
import { id, openSession, session } from "./nfr1";
import { cover, perfSamples } from "./perf";

const MINUTES = 10;
type Back = Window & { __perfBack?: number };

test(
  "PERF-ui-occluded: ten minutes streaming hidden, then shown",
  { tag: "@perf" },
  async ({ cupper }) => {
    test.setTimeout((MINUTES + 10) * 60_000);
    await cupper.launch({ throttled: true });
    await cupper.connected();
    const stream = await session(cupper, "say s8");
    const said = await untilEvent(cupper, stream, (e) => e.type === "turn.completed");
    await openSession(cupper, "say s8");
    const cdp = await cupper.window.context().newCDPSession(cupper.window);
    await cdp.send("Emulation.setFocusEmulationEnabled", { enabled: false });
    const visibility = async () => {
      await cupper.window.waitForTimeout(2000);
      return cupper.window.evaluate(() => document.visibilityState);
    };
    await cover(cupper, { stopFrames: false });
    let how = "covered";
    if ((await visibility()) !== "hidden") {
      how = "minimized";
      await (await cupper.app.browserWindow(cupper.window)).evaluate((w) => w.minimize());
    }
    const state = await visibility();
    test.info().annotations.push({ type: "occlusion", description: `${how}: ${state}` });
    if (state !== "hidden") {
      console.log(`perf ui_occluded: the environment does not qualify (${state} when ${how})`);
      return;
    }
    await cupper.cli.ok(["send", id(stream), `flood_rate 100 ${MINUTES * 60 + 60} 67`]);
    await untilEvent(cupper, stream, (e) => e.type === "item.delta" && e.seq > said.seq);
    const resets = await cupper.window.evaluate(() => window.__cupperGate.resets());
    await cupper.window.waitForTimeout(MINUTES * 60_000);
    // A frame asked for while hidden comes once the page shows again.
    await cupper.window.evaluate(() =>
      requestAnimationFrame((at) => ((window as Back).__perfBack = performance.timeOrigin + at)),
    );
    const shown = await cupper.app.evaluate(({ BrowserWindow }) => {
      for (const w of BrowserWindow.getAllWindows()) {
        if (!w.isFocusable()) w.close();
        else if (w.isMinimized()) w.restore();
      }
      return Date.now();
    });
    const back = await cupper.window.waitForFunction(() => (window as Back).__perfBack);
    const reset = (await cupper.window.evaluate(() => window.__cupperGate.resets())) - resets;
    perfSamples("ui_occluded_first_frame", "ms", [
      Math.round(((await back.jsonValue()) as number) - shown),
    ]);
    perfSamples("ui_occluded_resets", "resets", [reset]);
  },
);
