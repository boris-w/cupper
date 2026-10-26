import { describe, expect, test } from "vitest";

import { bundled } from "./bundle.ts";
import { allowedLicense, checkEntry } from "./gate.ts";
import { parseLockfile } from "./lockfile.ts";
import { checkRuntime } from "./runtime.ts";

const ALLOW = new Set(["MIT", "ISC", "Apache-2.0"]);

/** A workspace: the app bundles ui-core, which depends on left-pad, which depends on Anthropic's SDK. */
const LOCKFILE = `---
lockfileVersion: '9.0'
importers:
  apps/desktop:
    devDependencies:
      '@cupper/ui-core': {specifier: 'workspace:*', version: link:../../packages/ui-core}
      '@anthropic-ai/sdk': {specifier: 0.1.0, version: 0.1.0}
      vite: {specifier: 8.3.3, version: 8.3.3}
  packages/ui-core:
    dependencies:
      left-pad: {specifier: 1.3.0, version: 1.3.0}
snapshots:
  left-pad@1.3.0:
    dependencies:
      '@anthropic-ai/sdk': 0.1.0
  '@anthropic-ai/sdk@0.1.0': {}
  vite@8.3.3:
    dependencies:
      lightningcss: 1.30.1
  lightningcss@1.30.1: {}
`;

const graph = parseLockfile(LOCKFILE);
const LEFT_PAD = "node_modules/.pnpm/left-pad@1.3.0/node_modules/left-pad";
const SDK = "node_modules/.pnpm/@anthropic-ai+sdk@0.1.0/node_modules/@anthropic-ai/sdk";
const licenses: Record<string, string> = { [LEFT_PAD]: "MIT", [SDK]: "SEE LICENSE IN README.md" };

const entry = (name: string, modules: string[]) => ({
  name,
  root: "apps/desktop",
  bundled: bundled([{ file: "index.js", modules }], graph.importers),
});
const check = (e: ReturnType<typeof entry>) => checkEntry(e, graph, (dir) => licenses[dir], ALLOW);

describe("UNIT-license-gate-negative", () => {
  test("Anthropic's SDK in an entry's closure fails with its path, bundled or shaken out", () => {
    const renderer = entry("renderer", [
      "apps/desktop/src/renderer/main.ts",
      "packages/ui-core/src/index.ts",
      `${LEFT_PAD}/index.js`,
      "\0vite/preload-helper.js",
    ]);
    expect(check(renderer)).toEqual([
      "renderer: apps/desktop → packages/ui-core → left-pad → @anthropic-ai/sdk (ADR-11)",
    ]);
    // Bundled from the app's own devDependencies: the path says so, and its license fails too.
    const main = entry("main", ["apps/desktop/src/main/index.ts", `${SDK}/index.js`]);
    expect(check(main)).toEqual([
      "main: apps/desktop → @anthropic-ai/sdk (ADR-11)",
      "main: @anthropic-ai/sdk@0.1.0 bundles under SEE LICENSE IN README.md, not in the allowlist",
    ]);
  });

  test("what only the build uses does not count: vite's lightningcss is not in the closure", () => {
    expect(check(entry("preload", ["apps/desktop/src/preload/index.ts"]))).toEqual([]);
  });

  test("a license expression passes on either side of OR and on both of AND", () => {
    expect(allowedLicense("(MIT OR GPL-3.0)", ALLOW)).toBe(true);
    expect(allowedLicense("MIT AND ISC", ALLOW)).toBe(true);
    expect(allowedLicense("MIT AND MPL-2.0", ALLOW)).toBe(false);
    expect(allowedLicense("Python-2.0", ALLOW)).toBe(false);
  });

  test("a runtime component without its license text fails, and so do stale codex-acp notices", () => {
    const shipped = { url: "https://example.invalid/x.tgz" };
    const manifest = { node: shipped, codex_acp: shipped, esbuild_wasm: shipped, bun: shipped };
    const notices = "# notices\n\n## a@1.0.0 (MIT)\n\ntext\n\n## gone@3.0.0 (MIT)\n\ntext\n";
    const texts: Record<string, string> = {
      "node-LICENSE": "text",
      "codex-acp-LICENSE": "text",
      "codex-acp-THIRD_PARTY_NOTICES.md": notices,
    };
    const lock = JSON.stringify({
      packages: {
        "": { name: "codex-acp" },
        "node_modules/a": { version: "1.0.0", license: "MIT" },
        "node_modules/b": { version: "2.0.0", license: "ISC" },
        "node_modules/@anthropic-ai/sdk": { version: "0.1.0", license: "MIT" },
        "node_modules/@openai/codex": { version: "0.160.1", license: "Apache-2.0" },
        "node_modules/typescript": { version: "6.0.3", license: "Apache-2.0", dev: true },
      },
    });
    expect(checkRuntime(manifest, (file) => texts[file], lock, ALLOW)).toEqual([
      "runtime: esbuild_wasm has no runtime/licenses/esbuild-wasm-LICENSE.md",
      "runtime: bun ships, but the gate knows no license text of it",
      "runtime: the codex-acp notices have no text for b@2.0.0",
      "runtime: codex-acp → @anthropic-ai/sdk (ADR-11)",
      "runtime: the codex-acp notices have no text for @anthropic-ai/sdk@0.1.0",
      "runtime: the codex-acp notices name gone@3.0.0, which codex-acp does not inline",
    ]);
  });
});
