// The shortcuts of design 5.7.7 (UI-4, T153), through the menu items that hold
// their accelerators: the sidebar's rows, ⌃1–9, the recent sessions, the model
// menu and ⌘.; and ⌘⌥A, the next session that needs action (PS-4). Rename,
// archive and "在终端中打开" are T265's.

import { expect, test, type Cupper } from "./fixture";
import { texts, untilEvent } from "./helpers";
import { focusWindow, requestPanel, shortcut } from "./input";
import { asked, id, session } from "./nfr1";

type Text = Awaited<ReturnType<typeof texts>>;
type Released = Window & { e2eReleased?: boolean };

/** The sidebar's row of the session titled `title`. */
const row = (cupper: Cupper, t: Text, title: string) =>
  cupper.window
    .getByRole("navigation", { name: t("shell:sidebar") })
    .getByRole("button", { name: new RegExp(title) });

/** Sessions made in turn, each once its turn is over: the last is the sidebar's first. */
async function sessions(cupper: Cupper, scripts: string[]): Promise<string[]> {
  const made = [];
  for (const script of scripts) {
    const stream = await session(cupper, script);
    await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
    made.push(stream);
  }
  return made;
}

/** Runs the shortcut, then expects the session titled `title` on screen. */
async function lands(cupper: Cupper, t: Text, accelerator: string, title: string) {
  await shortcut(cupper, accelerator);
  await expect(row(cupper, t, title)).toHaveAttribute("aria-current", "page");
}

/** Lets ⌃ go, which ends a ⌃Tab walk; returns once the page heard it. */
async function releaseControl(cupper: Cupper) {
  await cupper.window.evaluate(() => {
    const page = window as Released;
    page.e2eReleased = false;
    window.addEventListener("keyup", (event) => {
      if (event.key === "Control") page.e2eReleased = true;
    });
  });
  const main = await cupper.app.browserWindow(cupper.window);
  await main.evaluate((w) => w.webContents.sendInputEvent({ type: "keyUp", keyCode: "Control" }));
  await cupper.window.waitForFunction(() => (window as Released).e2eReleased);
}

test(
  "E2E-shortcuts: the sidebar's rows, ⌃1–9, the recent sessions, the model menu and ⌘.",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const [alpha] = await sessions(cupper, ["say alpha", "say bravo", "say charlie"]);
    await focusWindow(cupper);
    // The sidebar, most recent first: charlie, bravo, alpha.
    await lands(cupper, t, "Control+1", "say charlie");
    await lands(cupper, t, "Command+Alt+]", "say bravo");
    await lands(cupper, t, "Command+Alt+Right", "say alpha");
    await lands(cupper, t, "Command+Alt+[", "say bravo");
    // Seen last: bravo, alpha, charlie. With ⌃ held, ⌃Tab walks on through
    // them and ⌃⇧Tab back; only where ⌃ comes up counts as a visit.
    await lands(cupper, t, "Control+Tab", "say alpha");
    await lands(cupper, t, "Control+Tab", "say charlie");
    await lands(cupper, t, "Control+Shift+Tab", "say alpha");
    await releaseControl(cupper);
    await lands(cupper, t, "Control+Tab", "say bravo");
    await releaseControl(cupper);
    await lands(cupper, t, "Control+Tab", "say alpha");
    await releaseControl(cupper);

    // On alpha: ⌃⇧M opens the model menu and closes it; ⌘. stops the turn.
    await shortcut(cupper, "Control+Shift+M");
    const menu = cupper.window.getByRole("menu");
    await expect(menu.getByText(t("composer:model_title"))).toBeVisible();
    await shortcut(cupper, "Control+Shift+M");
    await expect(menu).toHaveCount(0);
    await cupper.cli.ok(["send", id(alpha as string), "wait_cancel"]);
    const running = t("sidebar:running");
    await expect(row(cupper, t, "say alpha").getByRole("img", { name: running })).toBeVisible();
    await shortcut(cupper, "Command+.");
    await untilEvent(
      cupper,
      alpha as string,
      (event) => event.type === "turn.completed" && event.data.status === "cancelled",
    );
  },
);

test(
  "E2E-next-needs-action: ⌘⌥A goes round the sessions that need action, a hidden row too",
  { tag: "@focus" },
  async ({ cupper }) => {
    await cupper.launch();
    await cupper.connected();
    const t = await texts(cupper);
    const first = await session(cupper, "permission k1");
    await asked(cupper, first);
    await sessions(
      cupper,
      [1, 2, 3, 4, 5].map((n) => `say idle ${n}`),
    );
    await asked(cupper, await session(cupper, "permission k2"));
    // The sidebar: k2, idle 5, 4, 3, 2; idle 1 and k1 behind "展开显示".
    await expect(row(cupper, t, "permission k1")).toHaveCount(0);
    await row(cupper, t, "say idle 3").click();
    await focusWindow(cupper);
    // After idle 3 comes k1: its project opens, so that its row shows.
    await lands(cupper, t, "Command+Alt+A", "permission k1");
    await expect(requestPanel(cupper)).toBeVisible();
    // Past the end, round to the start; and on.
    await lands(cupper, t, "Command+Alt+A", "permission k2");
    await expect(requestPanel(cupper)).toBeVisible();
    await lands(cupper, t, "Command+Alt+A", "permission k1");
  },
);
