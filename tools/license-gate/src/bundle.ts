/**
 * What one JS entry bundles, from the metafile the app's build writes
 * (`apps/desktop/dist/metafile/<environment>.json`): the modules of each
 * output, as paths relative to the repository.
 */

export interface Output {
  file: string;
  modules: string[];
}

export interface Bundled {
  /** Workspace packages whose sources the bundle holds. */
  importers: Set<string>;
  /** npm packages whose files the bundle holds, by `name@version`, with their directory. */
  packages: Map<string, { name: string; version: string; dir: string }>;
}

const STORE = /^((?:.*\/)?node_modules\/\.pnpm\/([^/]+)\/node_modules\/((?:@[^/]+\/)?[^/]+))\//;

/**
 * The packages of `outputs`' modules. A module in pnpm's store is its
 * package's, at the version its store directory names; any other file is the
 * workspace package's it lies in. Virtual modules (`\0…`) are the bundler's
 * own helpers, not a package's.
 */
export function bundled(outputs: Output[], importers: string[]): Bundled {
  const result: Bundled = { importers: new Set(), packages: new Map() };
  const workspace = importers
    .filter((path) => path !== ".")
    .toSorted((a, b) => b.length - a.length);
  for (const module of outputs.flatMap((output) => output.modules)) {
    if (module.startsWith("\0")) {
      continue;
    }
    const store = STORE.exec(module);
    if (store) {
      const [, dir = "", key = "", name = ""] = store;
      // `<name with + for />@<version>[_<peers>]`, perhaps shortened with a hash after `_`.
      const version = key.slice(name.length + 1).split("_")[0] ?? "";
      result.packages.set(`${name}@${version}`, { name, version, dir });
      continue;
    }
    const owner = workspace.find((path) => module.startsWith(`${path}/`));
    if (owner === undefined) {
      throw new Error(`${module} is in no workspace package and no package of the store`);
    }
    result.importers.add(owner);
  }
  return result;
}
