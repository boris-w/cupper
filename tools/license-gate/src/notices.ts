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

const LICENSE_FILE = /^(licen[cs]e|copying)([.-].*)?$/i;

async function licenseText(p: Inlined): Promise<string> {
  const response = await fetch(p.resolved);
  if (!response.ok) {
    throw new Error(`${p.resolved}: HTTP ${response.status}`);
  }
  const bytes = Buffer.from(await response.arrayBuffer());
  const integrity = `sha512-${createHash("sha512").update(bytes).digest("base64")}`;
  if (integrity !== p.integrity) {
    throw new Error(`${p.name}@${p.version}: the tarball does not match the lockfile's integrity`);
  }
  const dir = mkdtempSync(join(tmpdir(), "license-gate-"));
  try {
    writeFileSync(join(dir, "package.tgz"), bytes);
    execFileSync("tar", ["-xzf", "package.tgz"], { cwd: dir });
    // npm packs into `package/`, but a tarball may name its top directory otherwise.
    const top = readdirSync(dir).find((name) => name !== "package.tgz") ?? "";
    const file = readdirSync(join(dir, top)).find((name) => LICENSE_FILE.test(name));
    if (file === undefined) {
      throw new Error(`${p.name}@${p.version}: no license file in its tarball`);
    }
    return readFileSync(join(dir, top, file), "utf8");
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

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
