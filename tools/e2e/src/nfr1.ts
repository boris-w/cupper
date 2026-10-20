// E2E-NFR1's common steps (design 5.9.4): the window's and the CLI's view of
// a stream at the same seq, sessions made by the CLI, and a session on screen.

import type { Line, Output } from "./cli";
import { expect, type Cupper } from "./fixture";
import { events, texts, untilEvent } from "./helpers";

interface Followed {
  seq?: number;
  complete?: boolean;
  view?: unknown;
}

/** The window's view of `stream` (`index`, `session/<id>`), as `__cupperTest` exports it. */
async function windowView(cupper: Cupper, stream: string): Promise<Followed | undefined> {
  return cupper.window.evaluate(
    (stream) => (window.__cupperTest.views() as Record<string, Followed>)[stream],
    stream,
  );
}

/** A CLI that follows `stream` and prints its whole view after each batch (`cupper view`). */
export const follow = (cupper: Cupper, stream: string) =>
  cupper.cli.watch(["view", stream === "index" ? "index" : stream.replace("session/", "")]);

/**
 * Both views of `stream` once the window and the CLI reached the host's last
 * seq of it. Both subscribed from a snapshot and the window loaded no
 * history, so they hold the same window of the session.
 */
export async function bothViews(cupper: Cupper, stream: string, cli: Output) {
  let found: { window: unknown; cli: Line } | undefined;
  await expect
    .poll(
      async () => {
        const head = events(cupper, stream).at(-1)?.seq;
        const ui = await windowView(cupper, stream);
        const line = cli.lines.findLast((line) => line.seq === head);
        found =
          ui?.complete && ui.seq === head && line ? { window: ui.view, cli: line } : undefined;
        return found !== undefined;
      },
      { message: `the window and the CLI reach the head of ${stream}` },
    )
    .toBe(true);
  return found as { window: unknown; cli: Line };
}

/** A session the CLI made with `script` for the fake agent; its stream. */
export async function session(cupper: Cupper, script: string): Promise<string> {
  const args = ["new", "--agent", "claude", "--project", cupper.repo, script];
  const { session_id } = await cupper.cli.ok(args);
  return `session/${String(session_id)}`;
}

/** The session's id, from its stream. */
export const id = (stream: string) => stream.replace("session/", "");

/** Opens the session titled `title` from the sidebar. */
export async function openSession(cupper: Cupper, title: string) {
  await cupper.window.getByRole("button", { name: new RegExp(title) }).click();
}

/** The request the fake agent asked in `stream`, once asked; its id. */
export async function asked(cupper: Cupper, stream: string, after = 0): Promise<string> {
  const event = await untilEvent(
    cupper,
    stream,
    (event) => event.type === "interaction.requested" && event.seq > after,
  );
  return String(event.data.request_id);
}

/** A text of the app's with `{{value}}` as a pattern, for values the test does not know. */
export async function pattern(cupper: Cupper, key: string, value: string): Promise<RegExp> {
  const t = await texts(cupper);
  const [before = "", after = ""] = t(key, { [value]: "@@" }).split("@@");
  const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`${escape(before)}.+${escape(after)}`);
}
