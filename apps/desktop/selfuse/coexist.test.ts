// E2E-selfuse-coexist (T160): the Cupper Local that `cargo xtask ci selfuse`
// built runs beside the dev build, and each leaves the other alone (design
// 5.7.9): its own host, `host.lock`, data and logs.

import { existsSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { App, built, cdhash, cli, data, helper, HOST, HOST_APP, logs } from "./app";

describe("E2E-selfuse-coexist", () => {
  it("runs beside the dev build; every helper starts and the native extension loads", async () => {
    const own = new App(built("CUPPER_SELFUSE_APP"), "cupper-local");
    const dev = new App(built("CUPPER_DEV_APP"), "cupper-dev");
    try {
      await Promise.all([own.ready(), dev.ready()]);
      const host = own.host();
      expect(host).toMatchObject({
        exe: join(own.path, HOST),
        cdhash: cdhash(join(own.path, HOST_APP)),
      });
      expect(dev.host()).toMatchObject({ exe: join(dev.path, HOST) });
      expect(dev.host().pid).not.toBe(host.pid);
      // The Plugin helper starts for plugins only, which Electron 44 has
      // none of; IT-selfuse-build checks its signature.
      const helpers = ["", " (GPU)", " (Renderer)"].map(helper);
      expect(own.running()).toEqual(
        expect.arrayContaining(["Contents/MacOS/Cupper Local", ...helpers, HOST]),
      );
      for (const name of ["cupper-local", "cupper-dev"]) {
        const files = [
          join(data(name), "host.db"),
          join(data(name), "ui"),
          join(logs(name), "host.log"),
        ];
        expect(files.filter((file) => !existsSync(file))).toEqual([]);
      }

      await dev.quit();
      expect(own.host()).toEqual(host);
      const status = cli(own.path, "--json", "status");
      expect(status.status, status.stderr).toBe(0);
      expect(JSON.parse(status.stdout)).toMatchObject({ pending_upgrade: false, running_turns: 0 });
    } finally {
      await Promise.all([own.quit(), dev.quit()]);
    }
  });
});
