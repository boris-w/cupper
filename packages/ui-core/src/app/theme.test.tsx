// @vitest-environment jsdom
import { TRANSLUCENT_SIDEBAR_PREF } from "@cupper/shell-api";
import { createBrowserShell } from "@cupper/shell-api/browser-mock";
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, test } from "vitest";

import { ShellProvider, ThemeProvider } from "./providers";

afterEach(cleanup);

async function show(shell: "electron" | "browser", prefs: Record<string, unknown> = {}) {
  const { shell: api, controls } = createBrowserShell({ info: { shell, theme: "dark", prefs } });
  render(
    <ShellProvider api={api} info={await api.init()}>
      <ThemeProvider>{null}</ThemeProvider>
    </ShellProvider>,
  );
  return { api, controls, root: document.documentElement };
}

describe("UI-theme: the root's theme and translucency", () => {
  test("the theme the shell reports, and each change of it", async () => {
    const { controls, root } = await show("browser");
    expect(root.dataset.theme).toBe("dark");
    act(() => controls.setTheme("light"));
    expect(root.dataset.theme).toBe("light");
  });

  test("translucent in the app unless turned off; never in a browser, which has no material", async () => {
    const { api, root } = await show("electron");
    expect(root.hasAttribute("data-translucent")).toBe(true);
    await act(() => api.prefs.set(TRANSLUCENT_SIDEBAR_PREF, false));
    expect(root.hasAttribute("data-translucent")).toBe(false);
    cleanup();
    root.setAttribute("data-translucent", "");
    expect((await show("browser")).root.hasAttribute("data-translucent")).toBe(false);
  });
});
