// IT-selfuse-build (T160): the Cupper Local that `cargo xtask ci selfuse`
// built is the self-use build of design 5.7.9, file by file; its
// configuration takes release-config's scan (T161).

import { execFileSync } from "node:child_process";
import { closeSync, openSync, readdirSync, readSync, realpathSync } from "node:fs";
import { join, relative } from "node:path";

import { describe, expect, it } from "vitest";

import { scanApp } from "../src/release-scan";
import { built, cdhash, CLI, codesign, field, helper, HOST, HOST_APP, plist } from "./app";

const JIT = "com.apple.security.cs.allow-jit";
const LIBRARIES = "com.apple.security.cs.disable-library-validation";
const NODE = `${HOST_APP}/Contents/Helpers/node`;
const IDENTIFIERS: Record<string, string> = {
  "Contents/MacOS/Cupper Local": "dev.cupper.app.local",
  [helper("")]: "dev.cupper.app.local.helper",
  [helper(" (Renderer)")]: "dev.cupper.app.local.helper.Renderer",
  [helper(" (GPU)")]: "dev.cupper.app.local.helper.GPU",
  [helper(" (Plugin)")]: "dev.cupper.app.local.helper.Plugin",
  [HOST]: "dev.cupper.app.local.host",
  [CLI]: "dev.cupper.cli.local",
  [NODE]: "dev.cupper.runtime.node",
};

type Kind = "executable" | "library" | "universal";

/**
 * What each Mach-O carries (design 5.7.9, 5.10.2): ad hoc, the hardened
 * runtime. electron-builder signs the app and every other executable with
 * the same entitlements, and libraries with none; the host and the CLI keep
 * `host-bundle`'s signature, Node `runtime-pack`'s.
 */
function expected(file: string, kind: Kind) {
  const own = [HOST, CLI].includes(file) ? [] : file === NODE ? [JIT] : [JIT, LIBRARIES];
  const identifier = IDENTIFIERS[file];
  const entitlements = kind === "executable" ? own : [];
  return { flags: "adhoc,runtime", entitlements, ...(identifier && { identifier }) };
}

function signature(app: string, file: string) {
  const { details, out } = codesign(join(app, file), "--entitlements", "-", "--xml");
  const entitlements = Object.keys(out ? plist("-", out) : {}).sort();
  const flags = /flags=0x\w+\(([^)]*)\)/.exec(details)?.[1];
  const identifier = IDENTIFIERS[file] && field(details, "Identifier");
  return { flags, entitlements, ...(identifier && { identifier }) };
}

/** Every Mach-O in `app` (`realpath`ed), by its path in the app, and what it is. */
function machOs(app: string): Map<string, Kind> {
  const found = new Map<string, Kind>();
  for (const entry of readdirSync(app, { recursive: true, withFileTypes: true })) {
    if (!entry.isFile()) {
      continue;
    }
    // The walk goes through the frameworks' links too: `Versions/Current` is `A`.
    const file = realpathSync(join(entry.parentPath, entry.name));
    const head = Buffer.alloc(16);
    const fd = openSync(file, "r");
    readSync(fd, head, 0, 16, 0);
    closeSync(fd);
    // MH_MAGIC_64, MH_EXECUTE (2); FAT_MAGIC, as read little-endian.
    const magic = head.readUInt32LE(0);
    if (magic === 0xfeedfacf) {
      found.set(relative(app, file), head.readUInt32LE(12) === 2 ? "executable" : "library");
    } else if (magic === 0xbebafeca) {
      found.set(relative(app, file), "universal");
    }
  }
  return found;
}

describe("IT-selfuse-build", () => {
  const app = built("CUPPER_SELFUSE_APP");

  it("is Cupper Local, sealed whole, with no automation usage description", () => {
    execFileSync("codesign", ["--verify", "--deep", "--strict", app]);
    const info = plist(join(app, "Contents/Info.plist"));
    expect(info).toMatchObject({
      CFBundleIdentifier: "dev.cupper.app.local",
      CFBundleDisplayName: "Cupper Local",
      CFBundleURLTypes: [{ CFBundleURLSchemes: ["cupper-local"] }],
      LSMinimumSystemVersion: "15.0",
    });
    expect(info).not.toHaveProperty("NSAppleEventsUsageDescription");
  });

  it("signs every Mach-O, the helpers', the host's and Node's included, as design 5.7.9 says", () => {
    const files = machOs(app);
    expect([...files.keys()]).toEqual(expect.arrayContaining(Object.keys(IDENTIFIERS)));
    // arm64 only (Q6).
    expect([...files.values()]).not.toContain("universal");
    const each = (sign: (file: string, kind: Kind) => unknown) =>
      Object.fromEntries([...files].map(([file, kind]) => [file, sign(file, kind)]));
    expect(each((file) => signature(app, file))).toEqual(each(expected));
  });

  it("burns the fuses, cookie encryption off, and packs the asar as release-config scans it", async () => {
    expect(await scanApp(app, "self-use")).toEqual([]);
  });

  it("--build-info: a self-use host and CLI, the CLI with this host's cdhash", () => {
    const info = (file: string) =>
      JSON.parse(execFileSync(join(app, file), ["--build-info"], { encoding: "utf8" })) as unknown;
    expect(info(HOST)).toMatchObject({ features: ["dev-auth"], debug_assertions: false });
    expect(info(CLI)).toMatchObject({
      build: "self_use",
      features: ["self-use"],
      host_check: "cdhash",
      host_cdhash: cdhash(join(app, HOST_APP)),
      debug_assertions: false,
    });
    // The native extension proves the one-time key, as every keyed build's does.
    const native = { exports: {} as { buildInfo(): unknown } };
    process.dlopen(native, join(app, "Contents/Resources/native/cupper_native.node"));
    expect(native.exports.buildInfo()).toEqual({ features: ["dev-auth"], hostCheck: "key_proof" });
  });

  it("has the Dock icons of the other colours in Resources (appIcon, T252)", () => {
    const icons = readdirSync(join(app, "Contents/Resources/app-icons")).sort();
    expect(icons).toEqual(["ink.png", "matcha.png", "mocha.png", "night.png", "sky.png"]);
  });
});
