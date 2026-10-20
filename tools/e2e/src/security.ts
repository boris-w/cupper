// The security cases' common steps (design 5.9.4, 5.7.8).

import type { Cupper } from "./fixture";
import { events } from "./helpers";
import { asked, id } from "./nfr1";

/** Sends `method` as the window's own code would, through the main process. */
export const asWindow = (cupper: Cupper, method: string, params: Record<string, unknown>) =>
  cupper.window.evaluate(([method, params]) => window.__cupperTest.command(method, params), [
    method,
    params,
  ] as const);

/** Whether the host wrote an answer to `request`. */
export const resolved = (cupper: Cupper, stream: string, request: string) =>
  events(cupper, stream).some(
    (event) => event.type === "interaction.resolved" && event.data.request_id === request,
  );

/** A permission the fake agent asks in the session on screen; its request id. */
export async function permission(cupper: Cupper, stream: string, script = "permission p1") {
  const seq = events(cupper, stream).at(-1)?.seq ?? 0;
  await cupper.cli.ok(["send", id(stream), script]);
  return asked(cupper, stream, seq);
}

/** The window losing and regaining the focus, as the main process hears of it. */
export async function refocus(cupper: Cupper) {
  const window = await cupper.app.browserWindow(cupper.window);
  await window.evaluate((w) => {
    w.emit("blur");
    w.emit("focus");
  });
}
