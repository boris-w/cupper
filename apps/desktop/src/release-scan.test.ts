// UNIT-release-config-scan (T161): each check of the scan passes a release
// build's configuration and fails on a violation of each kind.

import { FuseState } from "@electron/fuses/dist/constants.js";
import { describe, expect, it } from "vitest";

import {
  asarFiles,
  asarViolations,
  buildInfoViolations,
  fuseViolations,
  hookViolations,
  plistViolations,
  usageDescriptions,
} from "./release-scan";
import { TEST_ENTRY_POINTS } from "./test-entry-points";

/** An asar archive of `files` (its header only): the header's size, then its JSON. */
function archive(files: Record<string, unknown>): Buffer {
  const json = Buffer.from(JSON.stringify({ files }));
  const head = Buffer.alloc(16);
  [4, json.length + 8, json.length + 4, json.length].forEach((n, at) =>
    head.writeUInt32LE(n, at * 4),
  );
  return Buffer.concat([head, json]);
}

const dir = (files: Record<string, unknown>) => ({ files });
const file = { size: 1, offset: "0" };

describe("UNIT-release-config-scan", () => {
  const { ENABLE: on, DISABLE: off } = FuseState;
  const release = {
    version: "1",
    0: off,
    1: on,
    2: off,
    3: off,
    4: on,
    5: on,
    6: off,
    7: off,
    8: on,
  };

  it("fuses: one off its column, one design 5.7.9 does not know, one unread", () => {
    expect(fuseViolations(release, "release")).toEqual([]);
    expect(fuseViolations({ ...release, 1: off }, "self-use")).toEqual([]);
    expect(fuseViolations(release, "self-use")).toEqual([
      "fuse EnableCookieEncryption is on, not off",
    ]);
    expect(fuseViolations({ ...release, 0: on, 9: on }, "release")).toEqual([
      "fuse RunAsNode is on, not off",
      "fuse 9 is not one design 5.7.9 knows",
    ]);
    // The ninth unread, which @electron/fuses 1.8 has no name for.
    expect(fuseViolations({ ...release, 8: undefined }, "release")).toEqual([
      "fuse WasmTrapHandlers is undefined, not on",
    ]);
  });

  it("asar: a file outside `files`, a source map, a test, a fixture, one unpacked", () => {
    const main = dir({ "index.js": file, "index.js.map": file });
    const renderer = dir({ "a.test.js": file, fixtures: dir({ "x.json": file }) });
    const review = dir({ "r.js": { ...file, unpacked: true } });
    const packed = archive({
      "package.json": file,
      dist: dir({ main, renderer, review }),
      src: dir({ "index.ts": file }),
    });
    expect([...asarFiles(packed).keys()]).toHaveLength(7);
    expect(asarViolations(asarFiles(packed))).toEqual([
      "dist/main/index.js.map does not belong in app.asar",
      "dist/renderer/a.test.js does not belong in app.asar",
      "dist/renderer/fixtures/x.json does not belong in app.asar",
      "dist/review/r.js is unpacked",
      "src/index.ts does not belong in app.asar",
    ]);
  });

  it("each test entry point", () => {
    expect(hookViolations("window.cupper = shell;")).toEqual([]);
    for (const name of TEST_ENTRY_POINTS) {
      expect(hookViolations(`globalThis.${name} = {};`)).toEqual([
        `the test entry point ${name} is in`,
      ]);
    }
  });

  it("Info.plist: no asar integrity; its usage descriptions, reported", () => {
    expect(plistViolations({ ElectronAsarIntegrity: {} })).toEqual([]);
    expect(plistViolations({})).toEqual(["Info.plist has no ElectronAsarIntegrity"]);
    const info = { CFBundleName: "Cupper", NSCameraUsageDescription: "camera" };
    expect(usageDescriptions(info)).toEqual(["NSCameraUsageDescription"]);
  });

  it("--build-info: debug assertions, a feature, the native extension's", () => {
    expect(buildInfoViolations({ debug_assertions: false, features: [] }, [])).toEqual([]);
    expect(buildInfoViolations({ debug_assertions: true, features: ["test-harness"] }, [])).toEqual(
      ["debug assertions are on", 'features ["test-harness"], not []'],
    );
    expect(buildInfoViolations({ features: [], hostCheck: "codesign" }, [])).toEqual([]);
    expect(buildInfoViolations({ features: ["dev-auth"] }, [])).toEqual([
      'features ["dev-auth"], not []',
    ]);
  });
});
