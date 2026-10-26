/**
 * `license-gate`: the license gate's first layer (ADR-11, design 5.1.6),
 * run by `xtask ci license` after the app's build has written its metafiles.
 *
 *     node tools/license-gate/src/main.ts [check | codex-notices]
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { parse } from "smol-toml";

import { bundled, type Output } from "./bundle.ts";
import { checkEntry } from "./gate.ts";
import { parseLockfile } from "./lockfile.ts";
import { codexNotices } from "./notices.ts";
import { checkRuntime } from "./runtime.ts";

const USAGE = `usage: license-gate [check | codex-notices]

  check          the app's JS entries and the runtime components (the default;
                 build the app first: \`vite build --mode release\` in apps/desktop)
  codex-notices  write runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md from
                 codex-acp's lockfile (downloads the packages' tarballs)`;

const root = resolve(import.meta.dirname, "../../..");
const read = (path: string) => readFileSync(join(root, path), "utf8");

/** The app's JS entries: the environments of `apps/desktop/vite.config.ts`. */
const ENTRIES = { main: "main", preload: "preload", renderer: "client" };

/** The runtime script imports nothing but Node's builtins: esbuild-wasm comes by path. */
const SCRIPT = "runtime/bundle-claude-acp.mjs";

function check(): string[] {
  const graph = parseLockfile(read("pnpm-lock.yaml"));
  const deny = parse(read("deny.toml")) as { licenses: { allow: string[] } };
  const allow = new Set(deny.licenses.allow);
  const license = (dir: string) => {
    const pkg = JSON.parse(read(join(dir, "package.json"))) as { license?: unknown };
    return typeof pkg.license === "string" ? pkg.license : undefined;
  };
  const problems: string[] = [];
  for (const [name, environment] of Object.entries(ENTRIES)) {
    const metafile = `apps/desktop/dist/metafile/${environment}.json`;
    if (!existsSync(join(root, metafile))) {
      throw new Error(`no ${metafile}: build the app first`);
    }
    const outputs = JSON.parse(read(metafile)) as Output[];
    const entry = { name, root: "apps/desktop", bundled: bundled(outputs, graph.importers) };
    problems.push(...checkEntry(entry, graph, license, allow));
    console.log(`${name}: ${entry.bundled.packages.size} npm packages bundled`);
  }
  for (const [, specifier] of read(SCRIPT).matchAll(/^import .* from "([^"]+)";$/gm)) {
    if (!specifier?.startsWith("node:")) {
      problems.push(`runtime: ${SCRIPT} imports ${specifier}, which the runtime does not ship`);
    }
  }
  const manifest = JSON.parse(read("runtime/manifest.json")) as {
    codex_acp: { version: string };
  } & Record<string, unknown>;
  const licenseText = (file: string) => {
    const path = `runtime/licenses/${file}`;
    return existsSync(join(root, path)) ? read(path) : undefined;
  };
  const codexLock = read(`runtime/locks/codex-acp-${manifest.codex_acp.version}.package-lock.json`);
  problems.push(...checkRuntime(manifest, licenseText, codexLock, allow));
  return problems;
}

const [command = "check", ...rest] = process.argv.slice(2);
if (rest.length > 0 || !["check", "codex-notices"].includes(command)) {
  console.error(USAGE);
  process.exit(2);
}
if (command === "codex-notices") {
  const { version } = (
    JSON.parse(read("runtime/manifest.json")) as { codex_acp: { version: string } }
  ).codex_acp;
  const lock = read(`runtime/locks/codex-acp-${version}.package-lock.json`);
  writeFileSync(
    join(root, "runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md"),
    await codexNotices(lock, version),
  );
} else {
  const problems = check();
  for (const problem of problems) {
    console.error(problem);
  }
  if (problems.length > 0) {
    process.exit(1);
  }
  console.log("license-gate: no problem");
}
