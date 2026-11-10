/**
 * The scan of a built app's configuration (design 5.1.6 `release-config`,
 * 5.7.9; T161): the fuses, the asar archive, Info.plist, the test entry
 * points and `--build-info`. `release-config` scans the release build,
 * IT-selfuse-build Cupper Local; each check is a function of what was read,
 * so that UNIT-release-config-scan can feed it a violation of each kind.
 */

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { getCurrentFuseWire } from "@electron/fuses";
import { FuseState } from "@electron/fuses/dist/constants.js";

import { TEST_ENTRY_POINTS } from "./test-entry-points";

export type Build = "release" | "self-use";

/**
 * The fuses by their place in the wire: design 5.7.8's eight, and Electron
 * 44's ninth, which @electron/fuses 1.8 has no name for, burned as Electron
 * ships it.
 */
const FUSES: [name: string, release: boolean][] = [
  ["RunAsNode", false],
  ["EnableCookieEncryption", true],
  ["EnableNodeOptionsEnvironmentVariable", false],
  ["EnableNodeCliInspectArguments", false],
  ["EnableEmbeddedAsarIntegrityValidation", true],
  ["OnlyLoadAppFromAsar", true],
  ["LoadBrowserProcessSpecificV8Snapshot", false],
  ["GrantFileProtocolExtraPrivileges", false],
  ["WasmTrapHandlers", true],
];

/** The fuses of `wire` (`getCurrentFuseWire`) that are not as design 5.7.9 has them for `build`. */
export function fuseViolations(wire: Record<string, unknown>, build: Build): string[] {
  const found = FUSES.flatMap(([name, release], at) => {
    // Cupper Local turns cookie encryption off.
    const on = release && !(build === "self-use" && name === "EnableCookieEncryption");
    const want = on ? FuseState.ENABLE : FuseState.DISABLE;
    return wire[at] === want ? [] : [`fuse ${name} is ${state(wire[at])}, not ${state(want)}`];
  });
  const known = new Set(["version", ...FUSES.map((_, at) => String(at))]);
  const unknown = Object.keys(wire).filter((key) => !known.has(key));
  return [...found, ...unknown.map((key) => `fuse ${key} is not one design 5.7.9 knows`)];
}

const state = (value: unknown) =>
  value === FuseState.ENABLE ? "on" : value === FuseState.DISABLE ? "off" : String(value);

/** Everything an app may carry in its asar archive (electron-builder.*.yml `files`). */
const PACKED = /^(package\.json|dist\/(main|preload|renderer|review)\/.+)$/;
/** What it never carries, wherever: source maps, tests, fixtures. */
const NEVER = /\.map$|\.test\.|(^|\/)(fixtures?|testdata|__tests__)\//;

interface Entry {
  files?: Record<string, Entry>;
  unpacked?: boolean;
}

/** The files of an asar archive by path, from its header; `true` for one kept unpacked. */
export function asarFiles(archive: Buffer): Map<string, boolean> {
  // A pickle of the header's size, then a pickle of the header's JSON.
  const header = JSON.parse(archive.toString("utf8", 16, 16 + archive.readUInt32LE(12)));
  const found = new Map<string, boolean>();
  const walk = (entry: Entry, at: string) => {
    for (const [name, child] of Object.entries(entry.files ?? {})) {
      if (child.files) {
        walk(child, `${at}${name}/`);
      } else {
        found.set(`${at}${name}`, child.unpacked === true);
      }
    }
  };
  walk(header as Entry, "");
  return found;
}

/** The files of the archive that do not belong, and those kept unpacked: `asarUnpack` stays empty. */
export function asarViolations(files: Map<string, boolean>): string[] {
  return [...files].flatMap(([path, unpacked]) => [
    ...(PACKED.test(path) && !NEVER.test(path) ? [] : [`${path} does not belong in app.asar`]),
    ...(unpacked ? [`${path} is unpacked`] : []),
  ]);
}

/** The test entry points that `code` names: release-like builds have none (design 5.10.7). */
export function hookViolations(code: string): string[] {
  const named = TEST_ENTRY_POINTS.filter((name) => code.includes(name));
  return named.map((name) => `the test entry point ${name} is in`);
}

/** What Info.plist lacks: the asar's integrity, which the integrity fuse checks. */
export function plistViolations(info: Record<string, unknown>): string[] {
  return "ElectronAsarIntegrity" in info ? [] : ["Info.plist has no ElectronAsarIntegrity"];
}

/** The usage descriptions of Info.plist, which design 5.10.2 wants gone: reported only (T161). */
export function usageDescriptions(info: Record<string, unknown>): string[] {
  return Object.keys(info).filter((key) => /^NS\w+UsageDescription$/.test(key));
}

/** A `--build-info` (or the native extension's `buildInfo()`) that is not of a release-like build with `features`. */
export function buildInfoViolations(info: Record<string, unknown>, features: string[]): string[] {
  return [
    ...((info.debug_assertions ?? false) === false ? [] : ["debug assertions are on"]),
    ...(JSON.stringify(info.features) === JSON.stringify(features)
      ? []
      : [`features ${JSON.stringify(info.features)}, not ${JSON.stringify(features)}`]),
  ];
}

/** A plist file as JSON; `-` reads `input`. */
export function plist(file: string, input?: string): Record<string, unknown> {
  const args = ["-convert", "json", "-o", "-", "--", file];
  return JSON.parse(execFileSync("plutil", args, { input, encoding: "utf8" })) as Record<
    string,
    unknown
  >;
}

/** Every violation of `build`'s configuration in the app at `app`. */
export async function scanApp(app: string, build: Build): Promise<string[]> {
  const resources = join(app, "Contents/Resources");
  const archive = readFileSync(join(resources, "app.asar"));
  const native = existsSync(join(resources, "native/cupper_native.node"));
  return [
    ...fuseViolations(await getCurrentFuseWire(app), build),
    ...asarViolations(asarFiles(archive)),
    ...(existsSync(join(resources, "app.asar.unpacked")) ? ["app.asar.unpacked exists"] : []),
    ...(native ? [] : ["the native extension is not in Resources/native"]),
    ...hookViolations(archive.toString("latin1")),
    ...plistViolations(plist(join(app, "Contents/Info.plist"))),
  ];
}
