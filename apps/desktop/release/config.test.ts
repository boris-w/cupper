// release-config (T161): the release build that `cargo xtask ci
// release-config` made without a Developer ID, checked as design 5.1.6 lists.
// An unknown switch ends the app before it makes any directory, so this
// runs on any Mac.

import { execFileSync, spawnSync } from "node:child_process";
import { realpathSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { buildInfoViolations, plist, scanApp, usageDescriptions } from "../src/release-scan";

const app = realpathSync(process.env.CUPPER_RELEASE_APP ?? "CUPPER_RELEASE_APP is not set");
const helpers = join(app, "Contents/Helpers");

describe("release-config", () => {
  it("burns the fuses and packs the asar of the release build; no test entry point", async () => {
    expect(await scanApp(app, "release")).toEqual([]);
    // Design 5.10.2 wants none; whether to remove Electron's is open (T161).
    const described = usageDescriptions(plist(join(app, "Contents/Info.plist")));
    console.info(`Info.plist's usage descriptions: ${described.join(" ") || "none"}`);
  });

  it("--build-info: the host, the CLI and the native extension without features", () => {
    const info = (file: string) =>
      JSON.parse(execFileSync(file, ["--build-info"], { encoding: "utf8" })) as Record<
        string,
        unknown
      >;
    expect(
      buildInfoViolations(info(join(helpers, "CupperHost.app/Contents/MacOS/cupper-host")), []),
    ).toEqual([]);
    expect(buildInfoViolations(info(join(helpers, "cupper")), [])).toEqual([]);
    const native = { exports: {} as { buildInfo(): Record<string, unknown> } };
    process.dlopen(native, join(app, "Contents/Resources/native/cupper_native.node"));
    expect(buildInfoViolations(native.exports.buildInfo(), [])).toEqual([]);
  });
});

describe("E2E-SEC-switches", () => {
  it("exits at once on a switch it does not know", () => {
    const run = spawnSync(join(app, "Contents/MacOS/Cupper"), ["--cupper-unknown-switch"], {
      encoding: "utf8",
      timeout: 30_000,
    });
    expect([run.status, run.stderr.trim()]).toEqual([
      1,
      "cupper: refusing to start with unknown switches: --cupper-unknown-switch",
    ]);
  });
});
