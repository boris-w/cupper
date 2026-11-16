import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

import { afterAll, describe, expect, test } from "vitest";

import { npmNotices, renderNotices, type Inputs } from "./third-party.ts";

const about = (id: string, text: string, ...crates: string[]) => ({
  licenses: [{ id, text, used_by: crates.map((name) => ({ crate: { name, version: "1.0.0" } })) }],
});

const inputs: Inputs = {
  runtime: {
    versions: { node: "24.0.0", codex_acp: "2.0.0", esbuild_wasm: "0.28.0" },
    text: (file) => `the text of ${file}`,
  },
  electron: { version: "45.0.0", text: "Electron's license" },
  npm: [{ id: "left-pad@1.3.0", license: "MIT", text: "left-pad's license" }],
  // The host and the CLI share serde and its license; the CLI alone has clap.
  crates: [
    about("MIT", "serde's license", "serde", "tokio"),
    about("MIT", "serde's license", "serde", "clap"),
  ],
};

const dirs: string[] = [];
afterAll(() => dirs.forEach((dir) => rmSync(dir, { recursive: true, force: true })));

/** A tree with `files` under it. */
function tree(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "license-gate-test-"));
  dirs.push(root);
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(dirname(join(root, path)), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

describe("UNIT-third-party-notices", () => {
  test("Node, codex-acp and what it inlines, esbuild-wasm, Electron, npm packages and Rust crates", () => {
    const notices = renderNotices(inputs);
    const sections = notices.split(/^={78}\n(.*)\n={78}$/m).filter((_, at) => at % 2 === 1);
    expect(sections).toEqual([
      "Node.js 24.0.0",
      "codex-acp 2.0.0, and the packages it inlines",
      "esbuild-wasm 0.28.0",
      "Electron 45.0.0",
      "npm packages in the app's JavaScript",
      "Rust crates of the host, the CLI and the native extension",
    ]);
    for (const text of [
      "the text of node-LICENSE",
      "the text of codex-acp-LICENSE",
      "the text of codex-acp-THIRD_PARTY_NOTICES.md",
      "the text of esbuild-wasm-LICENSE.md",
      "Electron's license",
      "---- left-pad@1.3.0 (MIT) ----\n\nleft-pad's license",
      // Once for every crate under the same text.
      "---- MIT: clap 1.0.0, serde 1.0.0, tokio 1.0.0 ----\n\nserde's license",
    ]) {
      expect(notices).toContain(text);
    }
    expect(notices.split("serde's license")).toHaveLength(2);
    expect(renderNotices({ ...inputs, crates: inputs.crates.toReversed() })).toBe(notices);
  });

  test("an npm package's text: its own, else the upstream one kept; without either, none is written", () => {
    const root = tree({
      "store/b/LICENSE.md": "b's license",
      "store/@scope/a/package.json": "{}",
      "tools/license-gate/licenses/@scope+a.txt": "a's upstream license",
    });
    const packages = [
      { name: "b", version: "2.0.0", dir: "store/b" },
      { name: "@scope/a", version: "1.0.0", dir: "store/@scope/a" },
    ];
    expect(npmNotices(root, packages, () => "MIT")).toEqual([
      { id: "@scope/a@1.0.0", license: "MIT", text: "a's upstream license" },
      { id: "b@2.0.0", license: "MIT", text: "b's license" },
    ]);
    rmSync(join(root, "tools/license-gate/licenses/@scope+a.txt"));
    expect(() => npmNotices(root, packages, () => "MIT")).toThrow(
      /^no license text in @scope\/a@1\.0\.0:/,
    );
  });
});
