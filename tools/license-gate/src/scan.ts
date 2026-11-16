/**
 * The license gate's second layer (ADR-11, design 5.10.7): the final
 * artifact, an `.app` or a zip of one, scanned by content for Anthropic's
 * SDK and Claude Code, and its notices checked; the DMG comes with T181.
 * Every file is matched: by hash against the SDK's files and the tarballs of
 * Anthropic's packages in claude-agent-acp's lockfile, by content against
 * the strings, and a Mach-O by the identifier and the team of its signature.
 * The lockfile the host bundle carries names the SDK, as any lockfile does:
 * it passes at its place when its sha-256 is the runtime manifest's.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";

import { IDENTIFIERS, SDK, STRINGS, TEAMS, walk, type Fingerprints } from "./fingerprints.ts";
import { forbidden } from "./gate.ts";
import { sri } from "./notices.ts";

export interface Known {
  /** What an SRI is of. */
  hashes: Map<string, string>;
  /** The sha-256 of each lockfile the host bundle carries, by its path in the bundle's `Contents`. */
  locks: Map<string, string>;
  /** The notices the app carries: this tree's. */
  notices: string;
  strings: string[];
  identifiers: string[];
  teams: string[];
}

/** What the scan knows, from the fingerprints, the lockfile and the runtime manifest's `files`. */
export function known(
  fingerprints: Fingerprints,
  lockText: string,
  files: Record<string, { sha256: string }>,
  notices: string,
): Known {
  const hashes = new Map(
    Object.entries(fingerprints.files).map(([path, hash]) => [hash, `${SDK}'s ${path}`]),
  );
  const lock = JSON.parse(lockText) as {
    packages: Record<string, { version?: string; integrity?: string }>;
  };
  for (const [key, p] of Object.entries(lock.packages)) {
    const name = key.slice(key.lastIndexOf("node_modules/") + "node_modules/".length);
    if (forbidden(name) && p.integrity)
      hashes.set(p.integrity, `the tarball of ${name}@${p.version}`);
  }
  const locks = Object.entries(files).filter(([path]) =>
    path.startsWith("Resources/runtime/locks/"),
  );
  return {
    hashes,
    locks: new Map(locks.map(([path, hash]) => [path, hash.sha256])),
    notices,
    strings: STRINGS,
    identifiers: IDENTIFIERS,
    teams: TEAMS,
  };
}

/** The problems of the artifact at `artifact`. */
export function scanArtifact(artifact: string, known: Known): string[] {
  const name = basename(artifact);
  if (name.endsWith(".app")) {
    return [...scanTree(artifact, name, known), ...checkNotices(artifact, name, known)];
  }
  if (!name.endsWith(".zip")) {
    throw new Error(`${artifact}: neither an .app nor a zip`);
  }
  const dir = mkdtempSync(join(tmpdir(), "license-gate-"));
  try {
    execFileSync("unzip", ["-q", artifact, "-d", dir]);
    const apps = readdirSync(dir).filter((entry) => entry.endsWith(".app"));
    return [
      ...scanTree(dir, name, known),
      ...(apps.length > 0 ? [] : [`${name}: holds no .app`]),
      ...apps.flatMap((app) => checkNotices(join(dir, app), `${name}/${app}`, known)),
    ];
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

function scanTree(dir: string, label: string, known: Known): string[] {
  return walk(dir).flatMap((path) =>
    scanFile(`${label}/${path}`, readFileSync(join(dir, path)), known),
  );
}

/** The problems of one file, at `path` in the artifact. */
export function scanFile(path: string, bytes: Buffer, known: Known): string[] {
  for (const [lock, sha256] of known.locks) {
    const hash = () => createHash("sha256").update(bytes).digest("hex");
    if (path.endsWith(`/Helpers/CupperHost.app/Contents/${lock}`) && hash() === sha256) return [];
  }
  const what = known.hashes.get(sri(bytes));
  const problems = what === undefined ? [] : [`${path}: is ${what} (ADR-11)`];
  for (const text of known.strings) {
    if (bytes.includes(text)) problems.push(`${path}: holds "${text}" (ADR-11)`);
  }
  let signatures: Signature[];
  try {
    signatures = codeSignatures(bytes);
  } catch {
    return [...problems, `${path}: a Mach-O whose signature cannot be read`];
  }
  for (const { identifier, team = "" } of signatures) {
    if (known.identifiers.includes(identifier) || known.teams.includes(team)) {
      problems.push(`${path}: signed as ${identifier} by team ${team || "none"} (ADR-11)`);
    }
  }
  return problems;
}

function checkNotices(app: string, label: string, known: Known): string[] {
  const file = join(app, "Contents/Resources/THIRD_PARTY_NOTICES.txt");
  if (!existsSync(file)) {
    return [`${label}: no Contents/Resources/THIRD_PARTY_NOTICES.txt`];
  }
  return readFileSync(file, "utf8") === known.notices
    ? []
    : [`${label}: THIRD_PARTY_NOTICES.txt is not this tree's (\`license-gate notices\`)`];
}

export interface Signature {
  identifier: string;
  team?: string;
}

/**
 * The code directories of a Mach-O, thin or universal, one per slice: none
 * for a file that is no Mach-O or a slice without a signature. A universal
 * header names its slices; Java's class files share its magic, but their
 * version, where the slice count would be, is 45 or more.
 */
export function codeSignatures(bytes: Buffer): Signature[] {
  const magic = bytes.length < 8 ? 0 : bytes.readUInt32BE(0);
  if ((magic === 0xcafebabe || magic === 0xcafebabf) && bytes.readUInt32BE(4) < 45) {
    const wide = magic === 0xcafebabf;
    return Array.from({ length: bytes.readUInt32BE(4) }, (_, at) => {
      const arch = 8 + at * (wide ? 32 : 20);
      const offset = wide ? Number(bytes.readBigUInt64BE(arch + 8)) : bytes.readUInt32BE(arch + 8);
      const size = wide ? Number(bytes.readBigUInt64BE(arch + 16)) : bytes.readUInt32BE(arch + 12);
      return slice(bytes.subarray(offset, offset + size));
    }).flat();
  }
  return slice(bytes);
}

/** `LC_CODE_SIGNATURE`'s super blob, and the code directory at its slot 0. */
function slice(macho: Buffer): Signature[] {
  if (macho.length < 32 || macho.readUInt32LE(0) !== 0xfeedfacf) return [];
  let command = 32;
  for (let left = macho.readUInt32LE(16); left > 0; left--) {
    if (macho.readUInt32LE(command) === 0x1d) {
      const blob = macho.subarray(macho.readUInt32LE(command + 8));
      if (blob.readUInt32BE(0) !== 0xfade0cc0) break;
      for (let index = 0; index < blob.readUInt32BE(8); index++) {
        if (blob.readUInt32BE(12 + index * 8) !== 0) continue;
        const directory = blob.subarray(blob.readUInt32BE(16 + index * 8));
        if (directory.readUInt32BE(0) !== 0xfade0c02) break;
        const text = (at: number) => directory.toString("utf8", at, directory.indexOf(0, at));
        const team = directory.readUInt32BE(8) >= 0x20200 ? directory.readUInt32BE(48) : 0;
        return [
          { identifier: text(directory.readUInt32BE(20)), team: team ? text(team) : undefined },
        ];
      }
      break;
    }
    command += macho.readUInt32LE(command + 4);
  }
  return [];
}
