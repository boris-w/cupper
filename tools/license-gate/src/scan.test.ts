import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { afterAll, describe, expect, test } from "vitest";

import { sri } from "./notices.ts";
import { known, scanArtifact } from "./scan.ts";

const TARBALL = Buffer.from("the SDK's tarball");
const LOCK = JSON.stringify({
  packages: {
    "node_modules/@anthropic-ai/claude-agent-sdk": { version: "1.0.0", integrity: sri(TARBALL) },
  },
});
const LOCK_PATH = "Resources/runtime/locks/claude-agent-acp-1.0.0.package-lock.json";
const SDK_FILE = Buffer.from("the SDK's code ".repeat(64));
const sha256 = (text: string) => createHash("sha256").update(text).digest("hex");
const scanned = known(
  { version: "1.0.0", integrity: sri(TARBALL), files: { "sdk.mjs": sri(SDK_FILE) } },
  LOCK,
  { [LOCK_PATH]: { sha256: sha256(LOCK) } },
  "the notices\n",
);

/** A Mach-O whose code directory names `identifier` and `team`, laid out as codesign lays it out. */
function macho(identifier: string, team: string): Buffer {
  const directory = Buffer.alloc(52);
  directory.writeUInt32BE(0xfade0c02, 0);
  directory.writeUInt32BE(0x20200, 8);
  directory.writeUInt32BE(52, 20);
  directory.writeUInt32BE(53 + identifier.length, 48);
  const strings = Buffer.from(`${identifier}\0${team}\0`);
  const blob = Buffer.alloc(20);
  [0xfade0cc0, 20 + 52 + strings.length, 1, 0, 20].forEach((n, at) =>
    blob.writeUInt32BE(n, at * 4),
  );
  const header = Buffer.alloc(48);
  [
    0xfeedfacf,
    0x0100000c,
    0,
    2,
    1,
    16,
    0,
    0,
    0x1d,
    16,
    48,
    blob.length + 52 + strings.length,
  ].forEach((n, at) => header.writeUInt32LE(n, at * 4));
  return Buffer.concat([header, blob, directory, strings]);
}

/** A universal binary of `slices`. */
function universal(...slices: Buffer[]): Buffer {
  const header = Buffer.alloc(8 + slices.length * 20);
  header.writeUInt32BE(0xcafebabe, 0);
  header.writeUInt32BE(slices.length, 4);
  let offset = header.length;
  slices.forEach((slice, at) => {
    header.writeUInt32BE(offset, 8 + at * 20 + 8);
    header.writeUInt32BE(slice.length, 8 + at * 20 + 12);
    offset += slice.length;
  });
  return Buffer.concat([header, ...slices]);
}

const HOST = "Contents/Helpers/CupperHost.app/Contents";
const dirs: string[] = [];
afterAll(() => dirs.forEach((dir) => rmSync(dir, { recursive: true, force: true })));

/** `Cupper.app` in a temporary directory: notices, the host's lockfile and a signed main binary, then `files`. */
function app(files: Record<string, string | Buffer> = {}): string {
  const dir = mkdtempSync(join(tmpdir(), "license-gate-test-"));
  dirs.push(dir);
  const all = {
    "Contents/Resources/THIRD_PARTY_NOTICES.txt": "the notices\n",
    [`${HOST}/${LOCK_PATH}`]: LOCK,
    "Contents/MacOS/Cupper": macho("dev.cupper.app", "CUPPERTEAM"),
    ...files,
  };
  for (const [path, content] of Object.entries(all)) {
    mkdirSync(dirname(join(dir, "Cupper.app", path)), { recursive: true });
    writeFileSync(join(dir, "Cupper.app", path), content);
  }
  return join(dir, "Cupper.app");
}

describe("UNIT-license-gate-l2", () => {
  test("an app of Cupper's own passes, the lockfile its host bundle carries included", () => {
    expect(scanArtifact(app(), scanned)).toEqual([]);
  });

  test("the SDK by its strings, its files and its tarballs, in whatever file", () => {
    const files = {
      "Contents/Resources/app.asar": 'import("@anthropic-ai/claude-agent-sdk")',
      "Contents/Resources/LICENSE.md": "© Anthropic PBC. All rights reserved.",
      "Contents/Resources/vendor/sdk.mjs": SDK_FILE,
      "Contents/Resources/cache/sdk.tgz": TARBALL,
    };
    expect(scanArtifact(app(files), scanned).toSorted()).toEqual([
      'Cupper.app/Contents/Resources/LICENSE.md: holds "Anthropic PBC. All rights reserved" (ADR-11)',
      'Cupper.app/Contents/Resources/app.asar: holds "@anthropic-ai/claude-agent-sdk" (ADR-11)',
      "Cupper.app/Contents/Resources/cache/sdk.tgz: is the tarball of @anthropic-ai/claude-agent-sdk@1.0.0 (ADR-11)",
      "Cupper.app/Contents/Resources/vendor/sdk.mjs: is @anthropic-ai/claude-agent-sdk's sdk.mjs (ADR-11)",
    ]);
  });

  test("a Mach-O signed as Claude Code, or by Anthropic's team, thin or universal", () => {
    const files = {
      "Contents/Helpers/claude": macho("com.anthropic.claude-code", "OTHERTEAM1"),
      "Contents/Helpers/tool": macho("com.example.tool", "Q6L2SF6YDW"),
      "Contents/Helpers/both": universal(
        macho("dev.cupper.x", "CUPPERTEAM"),
        macho("com.anthropic.claude-code", ""),
      ),
      "Contents/Helpers/broken": macho("dev.cupper.x", "CUPPERTEAM").subarray(0, 60),
    };
    expect(scanArtifact(app(files), scanned).toSorted()).toEqual([
      "Cupper.app/Contents/Helpers/both: signed as com.anthropic.claude-code by team none (ADR-11)",
      "Cupper.app/Contents/Helpers/broken: a Mach-O whose signature cannot be read",
      "Cupper.app/Contents/Helpers/claude: signed as com.anthropic.claude-code by team OTHERTEAM1 (ADR-11)",
      "Cupper.app/Contents/Helpers/tool: signed as com.example.tool by team Q6L2SF6YDW (ADR-11)",
    ]);
  });

  test("the lockfile passes only as the manifest pins it, and only at its place", () => {
    const changed = { [`${HOST}/${LOCK_PATH}`]: LOCK.replace("1.0.0", "1.0.1") };
    const moved = { "Contents/Resources/lock.json": LOCK };
    const holds = '"@anthropic-ai/claude-agent-sdk" (ADR-11)';
    expect(scanArtifact(app(changed), scanned)).toEqual([
      `Cupper.app/${HOST}/${LOCK_PATH}: holds ${holds}`,
    ]);
    expect(scanArtifact(app(moved), scanned)).toEqual([
      `Cupper.app/Contents/Resources/lock.json: holds ${holds}`,
    ]);
  });

  test("a zip of the app is scanned as the app is", () => {
    const path = app({ "Contents/Resources/sdk.mjs": SDK_FILE });
    const zip = join(dirname(path), "Cupper.zip");
    execFileSync("zip", ["-qry", zip, "Cupper.app"], { cwd: dirname(path) });
    expect(scanArtifact(zip, scanned)).toEqual([
      "Cupper.zip/Cupper.app/Contents/Resources/sdk.mjs: is @anthropic-ai/claude-agent-sdk's sdk.mjs (ADR-11)",
    ]);
  });

  test("the app's notices are this tree's", () => {
    const stale = { "Contents/Resources/THIRD_PARTY_NOTICES.txt": "older notices\n" };
    expect(scanArtifact(app(stale), scanned)).toEqual([
      "Cupper.app: THIRD_PARTY_NOTICES.txt is not this tree's (`license-gate notices`)",
    ]);
    const none = app();
    rmSync(join(none, "Contents/Resources/THIRD_PARTY_NOTICES.txt"));
    expect(scanArtifact(none, scanned)).toEqual([
      "Cupper.app: no Contents/Resources/THIRD_PARTY_NOTICES.txt",
    ]);
  });
});
