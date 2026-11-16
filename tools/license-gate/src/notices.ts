/**
 * The notices of the packages codex-acp's single file inlines (ADR-11, T004's
 * handoff): each package's tarball as its tag's lockfile resolves it, checked
 * against the integrity there, and the license text in it. Needs the network;
 * the maintainer runs it when codex-acp's version changes, the gate only
 * checks the result.
 */
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { inlined, type Inlined } from "./runtime.ts";

/** The license file of a package directory, by its name. */
export const licenseFile = (dir: string) =>
  readdirSync(dir).find((name) => /^(licen[cs]e|copying)([.-].*)?$/i.test(name));

/** npm's Subresource Integrity of `bytes`, as lockfiles record it. */
export const sri = (bytes: Buffer) =>
  `sha512-${createHash("sha512").update(bytes).digest("base64")}`;

/**
 * What `read` makes of the package at `resolved`: its tarball, checked
 * against `integrity`, unpacked in a temporary directory that is removed
 * however `read` ends.
 */
export async function fetchPackage<T>(
  resolved: string,
  integrity: string,
  read: (dir: string) => T,
): Promise<T> {
  const response = await fetch(resolved);
  if (!response.ok) {
    throw new Error(`${resolved}: HTTP ${response.status}`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  if (sri(bytes) !== integrity) {
    throw new Error(`${resolved}: the tarball does not match the lockfile's integrity`);
  }
  const dir = mkdtempSync(join(tmpdir(), "license-gate-"));
  try {
    writeFileSync(join(dir, "package.tgz"), bytes);
    execFileSync("tar", ["-xzf", "package.tgz"], { cwd: dir });
    rmSync(join(dir, "package.tgz"));
    // npm packs into `package/`, but a tarball may name its top directory otherwise.
    return read(join(dir, readdirSync(dir)[0] ?? ""));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const licenseText = (p: Inlined) =>
  fetchPackage(p.resolved, p.integrity, (dir) => {
    const file = licenseFile(dir);
    if (file === undefined) {
      throw new Error(`${p.name}@${p.version}: no license file in its tarball`);
    }
    return readFileSync(join(dir, file), "utf8");
  });

/** The notices file for codex-acp `version`, from its tag's `package-lock.json`. */
export async function codexNotices(lockText: string, version: string): Promise<string> {
  const parts = [
    `# codex-acp ${version}: notices of the packages it inlines`,
    "",
    `The single file of \`@agentclientprotocol/codex-acp\` ${version} inlines these packages, at the versions its \`v${version}\` tag's \`package-lock.json\` pins (\`runtime/locks/codex-acp-${version}.package-lock.json\`). Written by \`node tools/license-gate/src/main.ts codex-notices\`; do not edit.`,
    "",
  ];
  for (const p of inlined(lockText)) {
    const text = (await licenseText(p)).trimEnd();
    parts.push(`## ${p.name}@${p.version} (${p.license})`, "", "```text", text, "```", "");
  }
  return parts.join("\n");
}
