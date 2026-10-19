import { mkdtemp, readdir, readFile, stat } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { MainLog } from "./log";

async function logDir() {
  return join(await mkdtemp(join(tmpdir(), "cupper-log-")), "logs");
}

describe("main.log", () => {
  it("writes JSON Lines with the home directory as ~", async () => {
    const dir = await logDir();
    const log = new MainLog(dir, { home: "/Users/u" });
    log.info("started", { flavor: "dev", path: "/Users/u/Library/x", other: "/Users/uv" });
    log.error("failed at /Users/u");
    log.close();
    const lines = (await readFile(join(dir, "main.log"), "utf8")).trimEnd().split("\n");
    const entries = lines.map((line) => JSON.parse(line) as Record<string, unknown>);
    expect(entries[0]).toMatchObject({
      level: "info",
      msg: "started",
      flavor: "dev",
      path: "~/Library/x",
      other: "/Users/uv",
    });
    expect(entries[1]).toMatchObject({ level: "error", msg: "failed at ~" });
    expect(Date.parse(String(entries[0]?.time))).not.toBeNaN();
    expect((await stat(dir)).mode & 0o777).toBe(0o700);
    expect((await stat(join(dir, "main.log"))).mode & 0o777).toBe(0o600);
  });

  it("rotates by size and keeps the configured number of files", async () => {
    const dir = await logDir();
    const log = new MainLog(dir, { home: "/Users/u", maxBytes: 200, files: 3 });
    for (let n = 0; n < 20; n++) {
      log.info("line", { n });
    }
    log.close();
    expect((await readdir(dir)).sort()).toEqual(["main.log", "main.log.1", "main.log.2"]);
    const last = (await readFile(join(dir, "main.log"), "utf8")).trimEnd().split("\n").at(-1);
    expect(JSON.parse(last ?? "")).toMatchObject({ n: 19 });
    for (const name of await readdir(dir)) {
      expect((await stat(join(dir, name))).size, name).toBeLessThanOrEqual(200);
    }
  });
});
