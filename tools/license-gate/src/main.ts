/**
 * `license-gate`: the license gate (ADR-11). Its first layer (design 5.1.6),
 * `check`, runs in `xtask ci license` after the app's build has written its
 * metafiles; its second (design 5.10.7), `scan`, in `release-config` on the
 * app it built. `notices` writes THIRD_PARTY_NOTICES.txt for the release
 * build, and `license` writes it too, to know it can.
 *
 *     node tools/license-gate/src/main.ts [check | notices | scan <artifact>... | codex-notices | fingerprints]
 */
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { parse } from "smol-toml";

import { bundled, type Output } from "./bundle.ts";
import { checkFingerprints, sdkFingerprints, type Fingerprints } from "./fingerprints.ts";
import { checkEntry } from "./gate.ts";
import { parseLockfile, type Graph } from "./lockfile.ts";
import { codexNotices } from "./notices.ts";
import { checkRuntime } from "./runtime.ts";
import { known, scanArtifact } from "./scan.ts";
import { cargoAbout, npmNotices, renderNotices } from "./third-party.ts";

const USAGE = `usage: license-gate [check | notices | scan <artifact>... | codex-notices | fingerprints]

  check          the app's JS entries and the runtime components, and whether
                 the fingerprints are of the SDK the lockfile pins (the default;
                 build the app first: \`vite build --mode release\` in apps/desktop)
  notices        write target/notices/THIRD_PARTY_NOTICES.txt (build the app
                 first; needs cargo-about 0.9.3)
  scan           the artifacts, each an .app or a zip of one, for Anthropic's
                 SDK and Claude Code, and for this tree's notices
  codex-notices  write runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md from
                 codex-acp's lockfile (downloads the packages' tarballs)
  fingerprints   write tools/license-gate/fingerprints.json from the SDK that
                 claude-agent-acp's lockfile pins (downloads its tarball: on a
                 runner only)`;

const root = resolve(import.meta.dirname, "../../..");
const read = (path: string) => readFileSync(join(root, path), "utf8");

/** The app's JS entries: the environments of `apps/desktop/vite.config.ts`, and its Worker's bundle. */
const ENTRIES = {
  main: "main",
  preload: "preload",
  renderer: "client",
  review: "review",
  "review worker": "review-worker",
};

/** The runtime script imports nothing but Node's builtins: esbuild-wasm comes by path. */
const SCRIPT = "runtime/bundle-claude-acp.mjs";

const manifest = () =>
  JSON.parse(read("runtime/manifest.json")) as {
    node: { version: string };
    codex_acp: { version: string };
    esbuild_wasm: { version: string };
    claude_agent_acp: { lockfile: string };
    files: Record<string, { sha256: string }>;
  } & Record<string, unknown>;
const claudeLock = () => read(`runtime/${manifest().claude_agent_acp.lockfile}`);
const FINGERPRINTS = "tools/license-gate/fingerprints.json";
const allow = () => (parse(read("deny.toml")) as { licenses: { allow: string[] } }).licenses.allow;
const license = (dir: string) => {
  const pkg = JSON.parse(read(join(dir, "package.json"))) as { license?: unknown };
  return typeof pkg.license === "string" ? pkg.license : undefined;
};

/** What each JS entry bundles, from the metafiles of the app's build. */
function entries(graph: Graph) {
  return Object.entries(ENTRIES).map(([name, environment]) => {
    const metafile = `apps/desktop/dist/metafile/${environment}.json`;
    if (!existsSync(join(root, metafile))) {
      throw new Error(`no ${metafile}: build the app first`);
    }
    const outputs = JSON.parse(read(metafile)) as Output[];
    return { name, root: "apps/desktop", bundled: bundled(outputs, graph.importers) };
  });
}

function check(): string[] {
  const graph = parseLockfile(read("pnpm-lock.yaml"));
  const allowed = new Set(allow());
  const problems: string[] = [];
  for (const entry of entries(graph)) {
    problems.push(...checkEntry(entry, graph, license, allowed));
    console.log(`${entry.name}: ${entry.bundled.packages.size} npm packages bundled`);
  }
  for (const [, specifier] of read(SCRIPT).matchAll(/^import .* from "([^"]+)";$/gm)) {
    if (!specifier?.startsWith("node:")) {
      problems.push(`runtime: ${SCRIPT} imports ${specifier}, which the runtime does not ship`);
    }
  }
  const licenseText = (file: string) => {
    const path = `runtime/licenses/${file}`;
    return existsSync(join(root, path)) ? read(path) : undefined;
  };
  const codexLock = read(
    `runtime/locks/codex-acp-${manifest().codex_acp.version}.package-lock.json`,
  );
  problems.push(...checkRuntime(manifest(), licenseText, codexLock, allowed));
  const fingerprints = JSON.parse(read(FINGERPRINTS)) as Fingerprints;
  problems.push(...checkFingerprints(fingerprints, claudeLock()));
  return problems;
}

/** This tree's THIRD_PARTY_NOTICES.txt. */
function notices(): string {
  const { node, codex_acp, esbuild_wasm } = manifest();
  const packages = new Map(
    entries(parseLockfile(read("pnpm-lock.yaml"))).flatMap((entry) => [...entry.bundled.packages]),
  );
  const electron = "apps/desktop/node_modules/electron";
  return renderNotices({
    runtime: {
      versions: {
        node: node.version,
        codex_acp: codex_acp.version,
        esbuild_wasm: esbuild_wasm.version,
      },
      text: (file) => read(`runtime/licenses/${file}`),
    },
    electron: {
      version: (JSON.parse(read(`${electron}/package.json`)) as { version: string }).version,
      text: read(`${electron}/LICENSE`),
    },
    npm: npmNotices(root, [...packages.values()], license),
    crates: cargoAbout(root, allow()),
  });
}

const report = (problems: string[]) => {
  for (const problem of problems) {
    console.error(problem);
  }
  if (problems.length > 0) {
    process.exit(1);
  }
  console.log("license-gate: no problem");
};

const [command = "check", ...rest] = process.argv.slice(2);
const commands = ["check", "notices", "scan", "codex-notices", "fingerprints"];
if (!commands.includes(command) || (command === "scan") !== rest.length > 0) {
  console.error(USAGE);
  process.exit(2);
}
if (command === "codex-notices") {
  const { version } = manifest().codex_acp;
  const lock = read(`runtime/locks/codex-acp-${version}.package-lock.json`);
  writeFileSync(
    join(root, "runtime/licenses/codex-acp-THIRD_PARTY_NOTICES.md"),
    await codexNotices(lock, version),
  );
} else if (command === "fingerprints") {
  const fingerprints = await sdkFingerprints(claudeLock());
  writeFileSync(join(root, FINGERPRINTS), `${JSON.stringify(fingerprints, null, 2)}\n`);
} else if (command === "notices") {
  mkdirSync(join(root, "target/notices"), { recursive: true });
  writeFileSync(join(root, "target/notices/THIRD_PARTY_NOTICES.txt"), notices());
} else if (command === "scan") {
  const fingerprints = JSON.parse(read(FINGERPRINTS)) as Fingerprints;
  const scanned = known(fingerprints, claudeLock(), manifest().files, notices());
  report(rest.flatMap((artifact) => scanArtifact(resolve(artifact), scanned)));
} else {
  report(check());
}
