/**
 * The runtime components that ship in the host bundle (`runtime/manifest.json`,
 * ADR-08, ADR-11): each one's license texts in `runtime/licenses/`, and for
 * codex-acp the notices of the packages its single file inlines.
 */
import { allowedLicense, forbidden } from "./gate.ts";

/** The license texts each shipped component needs, by its key in the manifest. */
export const LICENSES: Record<string, string[]> = {
  node: ["node-LICENSE"],
  codex_acp: ["codex-acp-LICENSE", "codex-acp-THIRD_PARTY_NOTICES.md"],
  esbuild_wasm: ["esbuild-wasm-LICENSE.md"],
};

export interface Inlined {
  name: string;
  version: string;
  license: string;
  resolved: string;
  integrity: string;
}

interface PackageLock {
  packages: Record<string, Partial<Inlined> & { dev?: boolean; devOptional?: boolean }>;
}

/**
 * What codex-acp's single file inlines: the packages of its own tag's
 * `package-lock.json` outside dev, but for the Codex CLI it runs as a
 * separate program (T004's handoff).
 */
export function inlined(lockText: string): Inlined[] {
  const lock = JSON.parse(lockText) as PackageLock;
  return Object.entries(lock.packages)
    .filter(([key, p]) => key !== "" && !p.dev && !p.devOptional)
    .map(([key, p]) => ({
      name: key.slice(key.lastIndexOf("node_modules/") + "node_modules/".length),
      version: p.version ?? "",
      license: p.license ?? "",
      resolved: p.resolved ?? "",
      integrity: p.integrity ?? "",
    }))
    .filter((p) => !p.name.startsWith("@openai/codex"));
}

/** The `## name@version (license)` headings of the notices, and whether text follows each. */
export function noticed(notices: string): Map<string, boolean> {
  const sections = notices.split(/^## /m).slice(1);
  return new Map(
    sections.map((section) => {
      const [heading = "", ...body] = section.split("\n");
      return [heading.split(" ")[0] ?? "", body.join("\n").trim().length > 0];
    }),
  );
}

/** The runtime components' problems: a missing license text, notices out of date, a package not allowed. */
export function checkRuntime(
  manifest: Record<string, unknown>,
  text: (file: string) => string | undefined,
  codexLock: string,
  allow: Set<string>,
): string[] {
  const problems: string[] = [];
  const shipped = Object.entries(manifest).filter(
    ([, value]) => typeof value === "object" && value !== null && "url" in value,
  );
  for (const [component] of shipped) {
    const files = LICENSES[component];
    if (files === undefined) {
      problems.push(`runtime: ${component} ships, but the gate knows no license text of it`);
    }
    for (const file of files ?? []) {
      if (!text(file)?.trim()) {
        problems.push(`runtime: ${component} has no runtime/licenses/${file}`);
      }
    }
  }
  const notices = noticed(text("codex-acp-THIRD_PARTY_NOTICES.md") ?? "");
  const packages = inlined(codexLock);
  for (const p of packages) {
    const id = `${p.name}@${p.version}`;
    if (forbidden(p.name)) {
      problems.push(`runtime: codex-acp → ${p.name} (ADR-11)`);
    }
    if (!allowedLicense(p.license, allow)) {
      problems.push(`runtime: codex-acp inlines ${id} under ${p.license}, not in the allowlist`);
    }
    if (!notices.get(id)) {
      problems.push(`runtime: the codex-acp notices have no text for ${id}`);
    }
  }
  const expected = new Set(packages.map((p) => `${p.name}@${p.version}`));
  for (const id of notices.keys()) {
    if (!expected.has(id)) {
      problems.push(`runtime: the codex-acp notices name ${id}, which codex-acp does not inline`);
    }
  }
  return problems;
}
