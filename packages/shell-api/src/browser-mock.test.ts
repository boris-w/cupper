import { describe, expect, test } from "vitest";

import { createBrowserShell } from "./browser-mock";
import { commandHandler } from "./commands";
import { isKnownTarget } from "./navigation";
import type {
  NavigateTarget,
  PrivilegedAction,
  ShellAPI,
  ShellCommand,
  UpdateState,
} from "./shell-api";

/** Every member of `T`; the compiler rejects this list when it misses or invents one. */
type Members<T> = {
  [K in keyof T]-?: T[K] extends (...args: never[]) => unknown ? "function" : Members<T[K]>;
};

const SHELL_API_MEMBERS: Members<ShellAPI> = {
  init: "function",
  host: { open: "function", onConnectionState: "function" },
  requestPrivileged: "function",
  onCommand: "function",
  onNavigate: "function",
  openExternal: "function",
  pickDirectory: "function",
  system: { onThemeChange: "function" },
  window: { onFocusChange: "function" },
  prefs: { set: "function", onChange: "function" },
  updates: { onState: "function", installNow: "function" },
};

/** The paths where `actual` differs from `expected`: missing or extra keys, or non-functions. */
function shapeDiff(actual: unknown, expected: object, path = ""): string[] {
  if (typeof actual !== "object" || actual === null) {
    return [path || "(root)"];
  }
  const keys = new Set([...Object.keys(actual), ...Object.keys(expected)]);
  return [...keys].flatMap((key) => {
    const at = path ? `${path}.${key}` : key;
    const want: unknown = Reflect.get(expected, key);
    const got: unknown = Reflect.get(actual, key);
    if (want === "function") {
      return typeof got === "function" ? [] : [at];
    }
    return typeof want === "object" && want !== null ? shapeDiff(got, want, at) : [at];
  });
}

function record<T>(): { seen: T[]; push: (value: T) => void } {
  const seen: T[] = [];
  return { seen, push: (value) => seen.push(value) };
}

describe("UNIT-shell-mock", () => {
  test("implements every member of ShellAPI and nothing else", () => {
    const { shell } = createBrowserShell();
    expect(shapeDiff(shell, SHELL_API_MEMBERS)).toEqual([]);
  });

  test("init returns the browser defaults with overrides, as a copy", async () => {
    const { shell } = createBrowserShell({ info: { theme: "dark", prefs: { sidebar: 280 } } });
    const info = await shell.init();
    expect(info).toEqual({
      shell: "browser",
      flavor: "dev",
      appVersion: "0.0.0",
      locale: "zh-CN",
      theme: "dark",
      prefs: { sidebar: 280 },
      updateState: { status: "idle" },
      reviewDocumentUrl: "about:blank",
    });
    info.prefs["sidebar"] = 0;
    expect((await shell.init()).prefs).toEqual({ sidebar: 280 });
  });

  test("onCommand delivers commands until unsubscribed", () => {
    const { shell, controls } = createBrowserShell();
    const commands = record<ShellCommand>();
    const unsubscribe = shell.onCommand(commands.push);
    controls.command({ id: "session_n", arg: 3 });
    unsubscribe();
    unsubscribe();
    controls.command({ id: "settings" });
    expect(commands.seen).toEqual([{ id: "session_n", arg: 3 }]);
  });

  test("onCommand ignores unknown command IDs without an error", () => {
    const { shell, controls } = createBrowserShell();
    const handled: string[] = [];
    shell.onCommand(
      commandHandler({
        new_session: () => handled.push("new_session"),
        session_n: (arg) => handled.push(`session_n ${String(arg)}`),
      }),
    );
    for (const id of ["open_command_menu", "", "toString", "__proto__", "constructor"]) {
      expect(() => {
        controls.command({ id });
      }).not.toThrow();
    }
    controls.command({ id: "settings" });
    controls.command({ id: "session_n", arg: 2 });
    controls.command({ id: "new_session" });
    expect(handled).toEqual(["session_n 2", "new_session"]);
  });

  test("a listener removed by another one during an emit is not called", () => {
    const { shell, controls } = createBrowserShell();
    const calls: string[] = [];
    shell.onCommand(() => {
      calls.push("first");
      unsubscribeSecond();
    });
    const unsubscribeSecond = shell.onCommand(() => calls.push("second"));
    controls.command({ id: "settings" });
    expect(calls).toEqual(["first"]);
  });

  test("onNavigate delivers every target, and the UI ignores unknown kinds", () => {
    const { shell, controls } = createBrowserShell();
    const targets = record<NavigateTarget>();
    const sessions: string[] = [];
    shell.onNavigate(targets.push);
    shell.onNavigate((target) => {
      if (isKnownTarget(target) && target.kind === "session") {
        sessions.push(target.session_id);
      }
    });
    controls.navigate({ kind: "session", session_id: "s1", request_id: "r1" });
    for (const kind of ["archive", "toString", "__proto__"]) {
      expect(() => {
        controls.navigate({ kind });
      }).not.toThrow();
    }
    expect(targets.seen).toEqual([
      { kind: "session", session_id: "s1", request_id: "r1" },
      { kind: "archive" },
      { kind: "toString" },
      { kind: "__proto__" },
    ]);
    expect(sessions).toEqual(["s1"]);
  });

  test("openExternal takes http and https URLs only", () => {
    const { shell, controls } = createBrowserShell();
    for (const url of [
      "https://example.com/a?b=c",
      "http://example.com",
      "javascript:alert(1)",
      "file:///etc/hosts",
      "app://cupper/index.html",
      "//example.com",
      "example.com",
      "",
    ]) {
      shell.openExternal(url);
    }
    expect(controls.openedUrls).toEqual(["https://example.com/a?b=c", "http://example.com"]);
  });

  test("pickDirectory resolves to the picked directory, null when cancelled", async () => {
    const { shell, controls } = createBrowserShell();
    expect(await shell.pickDirectory()).toBeNull();
    controls.setPickedDirectory("/work/repo");
    expect(await shell.pickDirectory()).toBe("/work/repo");
  });

  test("system.onThemeChange follows the system theme, and init sees it", async () => {
    const { shell, controls } = createBrowserShell();
    const themes = record<string>();
    shell.system.onThemeChange(themes.push);
    controls.setTheme("dark");
    expect(themes.seen).toEqual(["dark"]);
    expect((await shell.init()).theme).toBe("dark");
  });

  test("window.onFocusChange reports focus with its time", () => {
    const { shell, controls } = createBrowserShell();
    const changes: [boolean, number][] = [];
    shell.window.onFocusChange((focused, at) => changes.push([focused, at]));
    controls.setFocus(true, 1_000);
    controls.setFocus(false, 2_500);
    expect(changes).toEqual([
      [true, 1_000],
      [false, 2_500],
    ]);
  });

  test("prefs.set stores a copy and notifies onChange", async () => {
    const { shell } = createBrowserShell();
    const changes: [string, unknown][] = [];
    shell.prefs.onChange((key, value) => changes.push([key, value]));
    const value = { width: 320 };
    await shell.prefs.set("sidebar", value);
    value.width = 0;
    expect(changes).toEqual([["sidebar", { width: 320 }]]);
    expect((await shell.init()).prefs).toEqual({ sidebar: { width: 320 } });
    await expect(shell.prefs.set("bad", () => 1)).rejects.toThrow();
  });

  test("updates.installNow installs a downloaded update only", async () => {
    const { shell, controls } = createBrowserShell();
    const states = record<UpdateState>();
    shell.updates.onState(states.push);
    shell.updates.installNow();
    controls.setUpdateState({ status: "ready", version: "1.2.0" });
    shell.updates.installNow();
    expect(states.seen).toEqual([
      { status: "ready", version: "1.2.0" },
      { status: "installing", version: "1.2.0" },
    ]);
    expect((await shell.init()).updateState).toEqual({ status: "installing", version: "1.2.0" });
  });

  test("host.open rejects without an injected host, and uses the injected one", async () => {
    await expect(createBrowserShell().shell.host.open()).rejects.toThrow("no host");
    const open = () => Promise.reject(new Error("injected"));
    const host = { open, onConnectionState: () => () => undefined };
    await expect(createBrowserShell({ host }).shell.host.open()).rejects.toThrow("injected");
  });

  test("requestPrivileged records a copy of each action and resolves to the set result", async () => {
    const { shell, controls } = createBrowserShell();
    const action: PrivilegedAction = { kind: "trust_project", project_id: "p_1" };
    expect(await shell.requestPrivileged(action)).toEqual({ status: "cancelled" });
    controls.setPrivilegedResult({ status: "confirmed", result: { project_id: "p_2" } });
    expect(await shell.requestPrivileged({ kind: "add_project" })).toEqual({
      status: "confirmed",
      result: { project_id: "p_2" },
    });
    expect(controls.privilegedActions).toEqual([action, { kind: "add_project" }]);
    expect(controls.privilegedActions[0]).not.toBe(action);
  });
});
