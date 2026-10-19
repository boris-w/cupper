import type { KnownCommandId, ShellCommand } from "@cupper/shell-api";
import type { MenuItemConstructorOptions } from "electron";

export interface MenuOptions {
  /** Text from the `command` namespace of `@cupper/i18n`, keyed by command ID. */
  label: (id: KnownCommandId, n?: number) => string;
  /** Sends a command to the UI (`ShellAPI.onCommand`). */
  onCommand: (cmd: ShellCommand) => void;
  /** Adds "Toggle Developer Tools" to the View menu; dev builds only. */
  devTools: boolean;
}

/**
 * The macOS menu (design 5.7.7). Every shortcut with ⌘ or ⌃ is an accelerator
 * here, so it works while the focus is in the review document too; the menu
 * only sends the command, and the UI decides what it does. A command with a
 * second shortcut gets a hidden item for it: macOS still fires the
 * accelerators of hidden items. There is no Services menu (design 5.7.8).
 *
 * The roles keep Electron's own titles and items. They are English until
 * `@cupper/i18n` has text for them.
 */
export function menuTemplate({
  label,
  onCommand,
  devTools,
}: MenuOptions): MenuItemConstructorOptions[] {
  const command = (
    id: KnownCommandId,
    keys: string[],
    arg?: number,
  ): MenuItemConstructorOptions[] =>
    keys.map((accelerator, index) => ({
      label: label(id, arg),
      accelerator,
      visible: index === 0,
      click: () => onCommand(arg === undefined ? { id } : { id, arg }),
    }));
  const separator: MenuItemConstructorOptions = { type: "separator" };
  const sessions = [1, 2, 3, 4, 5, 6, 7, 8, 9].flatMap((n) =>
    command("session_n", [`Control+${n}`], n),
  );

  return [
    {
      role: "appMenu",
      submenu: [
        { role: "about" },
        separator,
        ...command("settings", ["Command+,"]),
        separator,
        { role: "hide" },
        { role: "hideOthers" },
        { role: "unhide" },
        separator,
        { role: "quit" },
      ],
    },
    {
      role: "fileMenu",
      submenu: [
        ...command("new_session", ["Command+N", "Command+Shift+O"]),
        separator,
        ...command("interrupt", ["Command+."]),
        ...command("model_picker", ["Control+Shift+M"]),
        separator,
        ...command("rename", ["Command+Alt+R"]),
        ...command("archive", ["Command+Shift+A"]),
        ...command("open_in_terminal", ["Command+Alt+T"]),
        separator,
        { role: "close" },
      ],
    },
    { role: "editMenu" },
    {
      role: "viewMenu",
      submenu: [
        ...command("toggle_sidebar", ["Command+B", "Command+Shift+S"]),
        ...command("toggle_side_panel", ["Command+Alt+B"]),
        ...command("toggle_file_tree", ["Command+Shift+E"]),
        ...command("open_review_tab", ["Control+Shift+G"]),
        separator,
        { role: "resetZoom" },
        { role: "zoomIn" },
        { role: "zoomOut" },
        separator,
        { role: "togglefullscreen" },
        ...(devTools ? [separator, { role: "toggleDevTools" } as const] : []),
      ],
    },
    {
      role: "windowMenu",
      submenu: [
        { role: "minimize" },
        { role: "zoom" },
        separator,
        ...command("next_needs_action", ["Command+Alt+A"]),
        ...command("prev_sidebar_item", ["Command+Alt+[", "Command+Alt+Left"]),
        ...command("next_sidebar_item", ["Command+Alt+]", "Command+Alt+Right"]),
        ...command("prev_recent", ["Control+Shift+Tab"]),
        ...command("next_recent", ["Control+Tab"]),
        separator,
        ...sessions,
        separator,
        { role: "front" },
      ],
    },
  ];
}
