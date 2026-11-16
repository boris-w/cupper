/**
 * THIRD_PARTY_NOTICES.txt (ADR-11, design 5.10.1, 5.10.7): the licenses of
 * what the app ships, in its `Contents/Resources`. The runtime components'
 * texts are `runtime/licenses/`; Electron's, its package's; each npm package
 * the app's JS bundles, the text in its package, or the upstream text in
 * `tools/license-gate/licenses/` for one that has none; the Rust crates of
 * the three binaries the app ships, cargo-about's.
 */
import { execFileSync } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { stringify } from "smol-toml";

import { licenseFile } from "./notices.ts";

export interface Inputs {
  /** Versions by the runtime manifest's keys, and a text of `runtime/licenses/` by its name. */
  runtime: {
    versions: Record<"node" | "codex_acp" | "esbuild_wasm", string>;
    text: (file: string) => string;
  };
  electron: { version: string; text: string };
  npm: { id: string; license: string; text: string }[];
  /** cargo-about's JSON, of each shipped crate. */
  crates: About[];
}

interface About {
  licenses: { id: string; text: string; used_by: { crate: { name: string; version: string } }[] }[];
}

const rule = "=".repeat(78);
const section = (title: string, ...texts: string[]) =>
  [rule, title, rule, "", ...texts.map((text) => `${text.trimEnd()}\n`)].join("\n");

export function renderNotices({ runtime, electron, npm, crates }: Inputs): string {
  const licenses = new Map<string, { id: string; text: string; used: Set<string> }>();
  for (const { id, text, used_by } of crates.flatMap((about) => about.licenses)) {
    const entry = licenses.get(`${id}\n${text}`) ?? { id, text, used: new Set() };
    for (const { crate } of used_by) entry.used.add(`${crate.name} ${crate.version}`);
    licenses.set(`${id}\n${text}`, entry);
  }
  const rust = [...licenses].toSorted(([a], [b]) => (a < b ? -1 : 1));
  return [
    "Cupper: third-party notices\n",
    "Cupper is licensed under the Apache License 2.0. It ships the third-party software",
    "below, each part followed by its license. Written by `license-gate notices`.\n",
    section(`Node.js ${runtime.versions.node}`, runtime.text("node-LICENSE")),
    section(
      `codex-acp ${runtime.versions.codex_acp}, and the packages it inlines`,
      runtime.text("codex-acp-LICENSE"),
      runtime.text("codex-acp-THIRD_PARTY_NOTICES.md"),
    ),
    section(
      `esbuild-wasm ${runtime.versions.esbuild_wasm}`,
      runtime.text("esbuild-wasm-LICENSE.md"),
    ),
    section(
      `Electron ${electron.version}`,
      electron.text,
      "Chromium's notices, and those of Electron's other parts, are in LICENSES.chromium.html,\nnext to this file.",
    ),
    section(
      "npm packages in the app's JavaScript",
      ...npm.map((p) => `---- ${p.id} (${p.license}) ----\n\n${p.text}`),
    ),
    section(
      "Rust crates of the host, the CLI and the native extension",
      ...rust.map(
        ([, l]) => `---- ${l.id}: ${[...l.used].toSorted().join(", ")} ----\n\n${l.text}`,
      ),
    ),
  ].join("\n");
}

/**
 * The license texts of the npm packages, by `name@version`, from their
 * directories under `root`: an error names every package without one.
 */
export function npmNotices(
  root: string,
  packages: { name: string; version: string; dir: string }[],
  license: (dir: string) => string | undefined,
): Inputs["npm"] {
  const missing: string[] = [];
  const found = packages.map((p) => {
    const file = licenseFile(join(root, p.dir));
    const upstream = join(root, "tools/license-gate/licenses", `${p.name.replace("/", "+")}.txt`);
    const path = file === undefined ? upstream : join(root, p.dir, file);
    const id = `${p.name}@${p.version}`;
    if (!existsSync(path)) missing.push(id);
    return {
      id,
      license: license(p.dir) ?? "",
      text: existsSync(path) ? readFileSync(path, "utf8") : "",
    };
  });
  if (missing.length > 0) {
    throw new Error(
      `no license text in ${missing.join(", ")}: put the upstream text in ` +
        "tools/license-gate/licenses/<name, / as +>.txt (ADR-11)",
    );
  }
  return found.toSorted((a, b) => (a.id < b.id ? -1 : 1));
}

/** The binaries the app ships, by their crates. */
const SHIPPED = ["cupper-host", "cupper-cli", "cupper-desktop-native"];

/**
 * cargo-about's JSON of each shipped crate, for the app's one target and
 * with the license allowlist of `deny.toml`; offline, so that the texts are
 * those of the crates as packaged and the same on every machine. Offline,
 * cargo reads the manifest of every crate in the lockfile, of any platform:
 * they are fetched first.
 */
export function cargoAbout(root: string, accepted: string[]): About[] {
  const dir = mkdtempSync(join(tmpdir(), "license-gate-"));
  try {
    const config = join(dir, "about.toml");
    writeFileSync(
      config,
      stringify({
        accepted,
        targets: ["aarch64-apple-darwin"],
        "ignore-build-dependencies": true,
        "ignore-dev-dependencies": true,
        // Cupper's own crates.
        private: { ignore: true },
      }),
    );
    execFileSync("cargo", ["fetch", "--locked"], {
      cwd: root,
      stdio: ["ignore", "ignore", "inherit"],
    });
    return SHIPPED.map((crate) => {
      const args = ["about", "generate", "--format", "json", "--frozen", "-c", config];
      const manifest = ["-m", `crates/${crate}/Cargo.toml`];
      const json = execFileSync("cargo", [...args, ...manifest], {
        cwd: root,
        encoding: "utf8",
        maxBuffer: 1 << 28,
        stdio: ["ignore", "pipe", "inherit"],
      });
      return JSON.parse(json) as About;
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}
