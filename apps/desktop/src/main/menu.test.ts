import type { KnownCommandId } from "@cupper/shell-api";
import type { MenuItemConstructorOptions } from "electron";
import { describe, expect, it, vi } from "vitest";

import { menuTemplate } from "./menu";

const COMMANDS: KnownCommandId[] = [
  "new_session",
  "settings",
  "toggle_sidebar",
  "toggle_side_panel",
  "open_review_tab",
  "toggle_file_tree",
  "interrupt",
  "next_needs_action",
  "prev_sidebar_item",
  "next_sidebar_item",
  "prev_recent",
  "next_recent",
  "session_n",
  "archive",
  "rename",
  "model_picker",
  "open_in_terminal",
];

function items(devTools = false) {
  const onCommand = vi.fn();
  const template = menuTemplate({ label: (id, n) => `${id}${n ?? ""}`, onCommand, devTools });
  const flat = (list: MenuItemConstructorOptions[]): MenuItemConstructorOptions[] =>
    list.flatMap((item) => [item, ...(Array.isArray(item.submenu) ? flat(item.submenu) : [])]);
  return { all: flat(template), onCommand };
}

describe("menu", () => {
  it("sends every known command, with its argument", () => {
    const { all, onCommand } = items();
    for (const item of all) {
      item.click?.({} as never, undefined, {} as never);
    }
    const sent = onCommand.mock.calls.map(([cmd]) => cmd as { id: string; arg?: number });
    expect(new Set(sent.map((cmd) => cmd.id))).toEqual(new Set(COMMANDS));
    const sessions = sent.filter((cmd) => cmd.id === "session_n").map((cmd) => cmd.arg);
    expect(sessions).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });

  it("gives each accelerator one item, and shows each command once", () => {
    const { all } = items();
    const accelerators = all.flatMap((item) => (item.accelerator ? [item.accelerator] : []));
    expect(new Set(accelerators).size).toBe(accelerators.length);
    const shown = all.filter((item) => item.click && item.visible !== false);
    expect(new Set(shown.map((item) => item.label)).size).toBe(shown.length);
    expect(all.find((item) => item.label === "new_session")?.accelerator).toBe("Command+N");
    expect(all.find((item) => item.label === "interrupt")?.accelerator).toBe("Command+.");
  });

  it("has no Services menu, and DevTools only when asked", () => {
    expect(items().all.some((item) => item.role === "services")).toBe(false);
    expect(items().all.some((item) => item.role === "toggleDevTools")).toBe(false);
    expect(items(true).all.some((item) => item.role === "toggleDevTools")).toBe(true);
  });
});
