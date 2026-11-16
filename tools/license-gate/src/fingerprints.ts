/**
 * What marks Anthropic's SDK and Claude Code for the license gate's second
 * layer (ADR-11, design 5.10.7). No file of either is ever kept, here or
 * anywhere: `fingerprints` downloads the SDK's tarball, as claude-agent-acp's
 * lockfile pins it, into a temporary directory on a runner, and writes only
 * the hashes of its files to `tools/license-gate/fingerprints.json`. The
 * strings, the signing identifier and the team are chosen here by hand.
 */
import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

import { fetchPackage, sri } from "./notices.ts";

/** The package whose files are fingerprinted. */
export const SDK = "@anthropic-ai/claude-agent-sdk";

/**
 * Strings of the SDK, in no file Cupper ships but the lockfile the host
 * bundle carries. The bundle script does not minify, so a string of the
 * SDK's code survives a bundle that holds it. Neither the environment
 * variables the SDK reads, which the host's runtime manifest lists, nor
 * `@anthropic-ai/claude-code`, which the host looks for on the user's
 * machine, can serve.
 */
export const STRINGS = [
  SDK,
  "Anthropic PBC. All rights reserved",
  "Claude Code process exited with code",
];

/**
 * Claude Code's signing identifier and team, public parts of its signature:
 * `codesign -dv` reads them from an installed Claude Code (2.1.296). Any
 * Mach-O Anthropic signed carries the team.
 */
export const IDENTIFIERS = ["com.anthropic.claude-code"];
export const TEAMS = ["Q6L2SF6YDW"];

/** The regular files under `dir`, relative to it; symbolic links are not followed. */
export const walk = (dir: string) =>
  readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile())
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)));

/** `fingerprints.json`: the SDK as the lockfile pins it, and the SRI of each of its files by path. */
export interface Fingerprints {
  version: string;
  integrity: string;
  files: Record<string, string>;
}

interface Lock {
  packages: Record<string, { version?: string; resolved?: string; integrity?: string }>;
}

const pinned = (lockText: string) =>
  (JSON.parse(lockText) as Lock).packages[`node_modules/${SDK}`] ?? {};

/** Whether the fingerprints are of the SDK claude-agent-acp's lockfile pins. */
export function checkFingerprints(fingerprints: Fingerprints, lockText: string): string[] {
  const { version, integrity } = pinned(lockText);
  return version === fingerprints.version && integrity === fingerprints.integrity
    ? []
    : [
        `fingerprints: of ${SDK}@${fingerprints.version}, but the lockfile pins ${version}; ` +
          "run `license-gate fingerprints` on a runner",
      ];
}

/** Files smaller than this are left out: too short to tell the SDK's from another's. */
const SMALLEST = 512;

/**
 * The fingerprints of the SDK the lockfile pins. Each string must be in its
 * files, and one at least in its code; where each was found is printed.
 */
export async function sdkFingerprints(lockText: string): Promise<Fingerprints> {
  const { version = "", resolved = "", integrity = "" } = pinned(lockText);
  const found = new Map(STRINGS.map((text) => [text, [] as string[]]));
  const files = await fetchPackage(resolved, integrity, (dir) =>
    walk(dir).flatMap((path) => {
      const bytes = readFileSync(join(dir, path));
      for (const [text, where] of found) {
        if (bytes.includes(text)) where.push(path);
      }
      return bytes.length < SMALLEST ? [] : [[path, sri(bytes)] as const];
    }),
  );
  for (const [text, where] of found) {
    console.log(`"${text}": ${where.join(", ") || "not found"}`);
  }
  const code = (where: string[]) => where.some((path) => /\.[cm]?js$/.test(path));
  if (![...found.values()].every((where) => where.length > 0)) {
    throw new Error(`a string is not in ${SDK}@${version}`);
  }
  if (![...found.values()].some(code)) {
    throw new Error(`no string is in the code of ${SDK}@${version}`);
  }
  return {
    version,
    integrity,
    files: Object.fromEntries(files.toSorted(([a], [b]) => (a < b ? -1 : 1))),
  };
}
