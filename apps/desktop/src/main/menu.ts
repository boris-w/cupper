import type { KnownCommandId, ShellCommand } from "@cupper/shell-api";
import type { MenuItemConstructorOptions } from "electron";

import type { ZoomStep } from "./zoom";

export interface MenuOptions {
  /** Text from the `command` namespace of `@cupper/i18n`, keyed by command ID. */
  label: (id: KnownCommandId, n?: number) => string;
  /** Text from the `menu` namespace, with the app's name: the menus' titles, the roles, the zoom. */
  text: (key: string) => string;
  /** Sends a command to the UI (`ShellAPI.onCommand`). */
  onCommand: (cmd: ShellCommand) => void;
  /** Adds "Toggle Developer Tools" to the View menu; dev builds only. */
  devTools: boolean;
  /** Zooms the focused window a step (zoom.ts). */
  onZoom: (step: ZoomStep) => void;
}

/**
 * The macOS menu (design 5.7.7), grouped as Codex's (26.1111, T153): the app
 * menu, File, Edit, View and Window. Every shortcut with ⌘ or ⌃ is an
 * accelerator here, so it works while the focus is in the review document
 * too; the menu only sends the command, and the UI decides what it does. A
 * command with a second shortcut gets a hidden item for it, and the commands
 * Codex keeps off its menus (rename, archive, the model, the nth session) are
 * hidden items only: macOS still fires the accelerators of hidden items.
 * There is no Services menu (design 5.7.8). Every title is cupper's text, the
 * roles' too. The zoom items are the roles' own, keys included (⌘0, ⌘+ and
 * ⌘=, ⌘−), but the menu zooms itself, so that the traffic lights follow the
 * title bar (zoom.ts).
 */
export function menuTemplate({
  label,
  text,
  onCommand,
  devTools,
  onZoom,
}: MenuOptions): MenuItemConstructorOptions[] {
  const command = (
    id: KnownCommandId,
    keys: string[],
    arg?: number,
    shown = true,
  ): MenuItemConstructorOptions[] =>
    keys.map((accelerator, index) => ({
      label: label(id, arg),
      accelerator,
      visible: shown && index === 0,
      click: () => onCommand(arg === undefined ? { id } : { id, arg }),
    }));
  const hidden = (id: KnownCommandId, keys: string[], arg?: number) =>
    command(id, keys, arg, false);
  const zoom = (step: ZoomStep, keys: string[]): MenuItemConstructorOptions[] =>
    keys.map((accelerator, index) => ({
      label: text(`zoom_${step}`),
      accelerator,
      visible: index === 0,
      click: () => onZoom(step),
    }));
  const role = (role: NonNullable<MenuItemConstructorOptions["role"]>, key: string) => ({
    role,
    label: text(key),
  });
  const separator: MenuItemConstructorOptions = { type: "separator" };
  const sessions = [1, 2, 3, 4, 5, 6, 7, 8, 9].flatMap((n) =>
    hidden("session_n", [`Control+${n}`], n),
  );

  return [
    {
      role: "appMenu",
      submenu: [
        role("about", "about"),
        separator,
        ...command("settings", ["Command+,"]),
        separator,
        role("hide", "hide"),
        role("hideOthers", "hide_others"),
        role("unhide", "show_all"),
        separator,
        role("quit", "quit"),
      ],
    },
    {
      label: text("file"),
      submenu: [
        ...command("new_session", ["Command+N", "Command+Shift+O"]),
        separator,
        ...command("interrupt", ["Command+."]),
        ...command("open_in_terminal", ["Command+Alt+T"]),
        separator,
        role("close", "close"),
      ],
    },
    {
      ...role("editMenu", "edit"),
      submenu: [
        role("undo", "undo"),
        role("redo", "redo"),
        separator,
        role("cut", "cut"),
        role("copy", "copy"),
        role("paste", "paste"),
        role("pasteAndMatchStyle", "paste_and_match_style"),
        role("delete", "delete"),
        separator,
        role("selectAll", "select_all"),
        separator,
        {
          label: text("substitutions"),
          submenu: [
            role("showSubstitutions", "show_substitutions"),
            separator,
            role("toggleSmartQuotes", "smart_quotes"),
            role("toggleSmartDashes", "smart_dashes"),
            role("toggleTextReplacement", "text_replacement"),
          ],
        },
        {
          label: text("speech"),
          submenu: [role("startSpeaking", "start_speaking"), role("stopSpeaking", "stop_speaking")],
        },
      ],
    },
    {
      label: text("view"),
      submenu: [
        ...command("toggle_sidebar", ["Command+B", "Command+Shift+S"]),
        ...command("toggle_side_panel", ["Command+Alt+B"]),
        ...command("toggle_file_tree", ["Command+Shift+E"]),
        ...command("open_review_tab", ["Control+Shift+G"]),
        separator,
        ...command("next_needs_action", ["Command+Alt+A"]),
        ...command("prev_sidebar_item", ["Command+Alt+[", "Command+Alt+Left"]),
        ...command("next_sidebar_item", ["Command+Alt+]", "Command+Alt+Right"]),
        separator,
        ...command("prev_recent", ["Control+Shift+Tab"]),
        ...command("next_recent", ["Control+Tab"]),
        separator,
        ...zoom("reset", ["CommandOrControl+0"]),
        ...zoom("in", ["CommandOrControl+Plus", "CommandOrControl+="]),
        ...zoom("out", ["CommandOrControl+-"]),
        separator,
        role("togglefullscreen", "toggle_full_screen"),
        ...(devTools ? [separator, role("toggleDevTools", "toggle_dev_tools")] : []),
        ...hidden("rename", ["Command+Alt+R"]),
        ...hidden("archive", ["Command+Shift+A"]),
        ...hidden("model_picker", ["Control+Shift+M"]),
        ...sessions,
      ],
    },
    {
      ...role("windowMenu", "window"),
      submenu: [
        role("minimize", "minimize"),
        role("zoom", "zoom"),
        separator,
        role("front", "front"),
      ],
    },
  ];
}
