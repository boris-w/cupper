import { mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it, onTestFinished, vi } from "vitest";

import { MAX_PREFS_BYTES, Prefs } from "./prefs";

/** A prefs file in a temporary directory that is removed when the test ends. */
async function prefsFile() {
  const dir = await mkdtemp(join(tmpdir(), "cupper-prefs-"));
  onTestFinished(() => rm(dir, { recursive: true, force: true }));
  return join(dir, "prefs.json");
}

describe("prefs", () => {
  it("persist JSON values, in order, readable only by the user", async () => {
    const file = await prefsFile();
    const prefs = await Prefs.open(file);
    await Promise.all([
      prefs.set("sidebar.width", 300),
      prefs.set("sidebar.width", 340),
      prefs.set("send_shortcut", "mod_enter"),
    ]);
    const reopened = await Prefs.open(file);
    expect(reopened.snapshot()).toEqual({ "sidebar.width": 340, send_shortcut: "mod_enter" });
    expect((await stat(file)).mode & 0o777).toBe(0o600);
  });

  it("start from the defaults when the file is missing or broken", async () => {
    const file = await prefsFile();
    expect((await Prefs.open(file)).snapshot()).toEqual({});
    await writeFile(file, "{ not json");
    expect((await Prefs.open(file)).snapshot()).toEqual({});
    await writeFile(file, "[1, 2]");
    expect((await Prefs.open(file)).snapshot()).toEqual({});
  });

  it("tell listeners after saving, until they unsubscribe", async () => {
    const prefs = await Prefs.open(await prefsFile());
    const listener = vi.fn();
    const unsubscribe = prefs.onChange(listener);
    await prefs.set("translucent_sidebar", false);
    unsubscribe();
    await prefs.set("translucent_sidebar", true);
    expect(listener.mock.calls).toEqual([["translucent_sidebar", false]]);
  });

  it("refuse odd keys, values that are not JSON and oversized files", async () => {
    const file = await prefsFile();
    const prefs = await Prefs.open(file);
    for (const key of ["", "__proto__", "constructor ", "a/b", "x".repeat(129)]) {
      await expect(prefs.set(key, 1), key).rejects.toThrow(TypeError);
    }
    await expect(prefs.set("f", () => 1)).rejects.toThrow(TypeError);
    await expect(prefs.set("big", 1n)).rejects.toThrow(TypeError);
    await expect(prefs.set("huge", "x".repeat(MAX_PREFS_BYTES))).rejects.toThrow(RangeError);
    expect(prefs.snapshot()).toEqual({});
    await expect(readFile(file)).rejects.toThrow(/ENOENT/);
  });
});
