// The security cases' common steps (design 5.9.4, 5.7.8).

import type { Line } from "./cli";
import type { Cupper } from "./fixture";
import { events } from "./helpers";
import { requestPanel } from "./input";
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

/** Activates the panel by moving the focus onto its options, which is no input. */
export async function focusOptions(cupper: Cupper) {
  await requestPanel(cupper)
    .getByRole("listbox")
    .evaluate((list: HTMLElement) => {
      list.blur();
      list.focus();
    });
}

/** The request's digest, from the window's view of the session. */
export async function digest(cupper: Cupper, stream: string, request: string) {
  const views = await cupper.window.evaluate(() => window.__cupperTest.views());
  const view = (views[stream] as { view: { requests: Record<string, { requested: Line }> } }).view;
  return view.requests[request]?.requested.digest;
}
