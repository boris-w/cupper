// Real input for the e2e tests (design 5.7.8; Q26, T077): Playwright's own
// keyboard never reaches `before-input-event`, so the input guard would not
// count it. Keys go through `webContents.sendInputEvent` instead, which the
// guard sees like the user's. Also the request panel's helpers (T083): wait
// until it takes keys, activate it with Tab, answer with a digit; and the
// menu's accelerators (T153).

import type { Locator } from "@playwright/test";

import { expect, type Cupper } from "./fixture";

/** Modifier names of `sendInputEvent`. */
export type Modifier = "shift" | "control" | "alt" | "meta";

/**
 * Gives the window the focus, then waits out the second in which the guard
 * counts no key after a window gains focus (its rule 7). A test that counts
 * on the focus, through keys or clicks as real input, is tagged `@focus`.
 *
 * The app takes the focus even from the active app, and the second counts
 * from when the window has it: a window that only asks may get it a while
 * later, on a loaded machine seconds later, and a key in the second after
 * that does not count (T189).
 */
export async function focusWindow(cupper: Cupper): Promise<void> {
  const window = await cupper.app.browserWindow(cupper.window);
  await cupper.app.evaluate(({ app }) => app.focus({ steal: true }));
  await window.evaluate((w) => {
    w.focus();
    w.webContents.focus();
  });
  await expect
    .poll(() => window.evaluate((w) => w.isFocused()), { message: "the window has the focus" })
    .toBe(true);
  await cupper.window.waitForTimeout(1100);
}

/**
 * Presses `key`: an Electron key code ("Tab", "Enter", "Escape", "Up", "1",
 * "a"), with modifiers; a one-character key also types its character.
 */
export async function press(cupper: Cupper, key: string, modifiers: Modifier[] = []) {
  const window = await cupper.app.browserWindow(cupper.window);
  await window.evaluate(
    (w, [key, modifiers]) => {
      w.webContents.sendInputEvent({ type: "keyDown", keyCode: key, modifiers });
      if ([...key].length === 1) {
        w.webContents.sendInputEvent({ type: "char", keyCode: key, modifiers });
      }
      w.webContents.sendInputEvent({ type: "keyUp", keyCode: key, modifiers });
    },
    [key, modifiers] as const,
  );
}

/**
 * Clicks the middle of `target` with the mouse through `sendInputEvent`,
 * which the guard counts as a click (mouseDown, then mouseUp). A native
 * button's Enter needs a `char` event besides the keys `press` sends, so a
 * button without its own key handling is clicked, not pressed.
 */
export async function click(cupper: Cupper, target: Locator) {
  const box = await target.boundingBox();
  if (!box) {
    throw new Error("nothing to click: the target is not on screen");
  }
  const at = { x: Math.round(box.x + box.width / 2), y: Math.round(box.y + box.height / 2) };
  const window = await cupper.app.browserWindow(cupper.window);
  await window.evaluate((w, at) => {
    for (const type of ["mouseDown", "mouseUp"] as const) {
      w.webContents.sendInputEvent({ type, ...at, button: "left", clickCount: 1 });
    }
  }, at);
}

/**
 * Runs the menu item that holds `accelerator`, as its keys do: the item sends
 * its command to the focused window, so the test is tagged `@focus`. Matching
 * the keys to the item is the system's (design 5.7.7).
 */
export async function shortcut(cupper: Cupper, accelerator: string) {
  const found = await cupper.app.evaluate(({ Menu }, accelerator) => {
    const items = [...(Menu.getApplicationMenu()?.items ?? [])];
    for (let item = items.pop(); item; item = items.pop()) {
      if (item.accelerator === accelerator) {
        item.click();
        return true;
      }
      items.push(...(item.submenu?.items ?? []));
    }
    return false;
  }, accelerator);
  expect(found, `a menu item holds ${accelerator}`).toBe(true);
}

/** Types `text` key by key. */
export async function type(cupper: Cupper, text: string) {
  for (const char of text) {
    await press(cupper, char);
  }
}

/** The request panel (T083). */
export const requestPanel = (cupper: Cupper) => cupper.window.locator("[data-request-panel]");

/** Once the panel's arming time is over and it takes keys and clicks (RP-1). */
export async function panelReady(cupper: Cupper) {
  await expect(requestPanel(cupper)).toHaveAttribute("data-ready", "true");
}

/**
 * Tabs into a panel that did not take the focus by itself (RP-3), until it
 * is active, along the page's focus order from where the focus is: the
 * sidebar, the title bar and the stream come first, however many stops they
 * have (T270). Once round that order without reaching the panel, it names
 * each stop it went through. Then waits out the second in which the guard
 * counts no digit after another key (its rule 3: Tab is such a key).
 */
export async function activatePanel(cupper: Cupper) {
  const panel = requestPanel(cupper);
  const stops: string[] = [];
  await cupper.window.evaluate(() => {
    (window as { tabbed?: WeakSet<Element> }).tabbed = new WeakSet();
  });
  while ((await panel.getAttribute("data-activation")) !== "auto") {
    await press(cupper, "Tab");
    // Where the Tab went, and whether the focus was there before.
    const { stop, again } = await cupper.window.evaluate(() => {
      const at = document.activeElement ?? document.body;
      const tabbed = (window as { tabbed?: WeakSet<Element> }).tabbed ?? new WeakSet();
      const again = tabbed.has(at);
      tabbed.add(at);
      const name = at.getAttribute("aria-label") ?? at.textContent?.trim().slice(0, 24);
      return { stop: name || at.tagName.toLowerCase(), again };
    });
    stops.push(stop);
    if (again) {
      throw new Error(`round the page without reaching the request panel: ${stops.join(" → ")}`);
    }
  }
  await cupper.window.waitForTimeout(1100);
}

/** Picks option `n` of the active panel by its number key, as real input. */
export const pressDigit = (cupper: Cupper, n: number) => press(cupper, String(n));
