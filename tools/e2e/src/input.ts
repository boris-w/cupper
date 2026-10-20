// Real input for the e2e tests (design 5.7.8; Q26, T077): Playwright's own
// keyboard never reaches `before-input-event`, so the input guard would not
// count it. Keys go through `webContents.sendInputEvent` instead, which the
// guard sees like the user's. Also the request panel's helpers (T083): wait
// until it takes keys, activate it with Tab, answer with a digit.

import { expect, type Cupper } from "./fixture";

/** Modifier names of `sendInputEvent`. */
export type Modifier = "shift" | "control" | "alt" | "meta";

/**
 * Gives the window the focus, then waits out the second in which the guard
 * counts no key after a window gains focus (its rule 7). A test that counts
 * on the focus, through keys or clicks as real input, is tagged `@focus`.
 */
export async function focusWindow(cupper: Cupper): Promise<void> {
  const window = await cupper.app.browserWindow(cupper.window);
  await window.evaluate((w) => {
    w.focus();
    w.webContents.focus();
  });
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
 * is active; then waits out the second in which the guard counts no digit
 * after another key (its rule 3: Tab is such a key).
 */
export async function activatePanel(cupper: Cupper) {
  const panel = requestPanel(cupper);
  for (let tabs = 0; (await panel.getAttribute("data-activation")) !== "auto"; tabs++) {
    if (tabs === 10) {
      throw new Error("ten tabs did not reach the request panel");
    }
    await press(cupper, "Tab");
  }
  await cupper.window.waitForTimeout(1100);
}

/** Picks option `n` of the active panel by its number key, as real input. */
export const pressDigit = (cupper: Cupper, n: number) => press(cupper, String(n));
