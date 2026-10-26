/**
 * The dependency graph of `pnpm-lock.yaml` (lockfile version 9). A node is a
 * workspace package, named by its path (`packages/ui-core`), or a package as
 * the lockfile resolves it, by its snapshot key (`react-dom@19.2.0(react@19.2.0)`).
 */
import { posix } from "node:path";

import { loadAll } from "js-yaml";

type Deps = Record<string, string | { version: string }>;

interface Raw {
  importers?: Record<string, Partial<Record<Kind, Deps>>>;
  snapshots?: Record<string, Partial<Record<Kind, Deps>>>;
}

type Kind = "dependencies" | "optionalDependencies";

export interface Graph {
  importers: string[];
  /** The production edges: `dependencies` and `optionalDependencies`. */
  prod: Map<string, string[]>;
}

const PROD: Kind[] = ["dependencies", "optionalDependencies"];

export function parseLockfile(text: string): Graph {
  // pnpm writes the package manager's own lockfile as a first document.
  const raw = (loadAll(text).at(-1) ?? {}) as Raw;
  const prod = new Map<string, string[]>();
  const targets = (from: string, deps: Deps | undefined) =>
    Object.entries(deps ?? {}).map(([name, ref]) => {
      const version = typeof ref === "string" ? ref : ref.version;
      // `link:` points at another workspace package, relative to this one.
      return version.startsWith("link:")
        ? posix.normalize(posix.join(from, version.slice("link:".length)))
        : `${name}@${version}`;
    });
  for (const [path, kinds] of Object.entries(raw.importers ?? {})) {
    prod.set(
      path,
      PROD.flatMap((kind) => targets(path, kinds[kind])),
    );
  }
  for (const [key, kinds] of Object.entries(raw.snapshots ?? {})) {
    prod.set(
      key,
      PROD.flatMap((kind) => targets(".", kinds[kind])),
    );
  }
  return { importers: Object.keys(raw.importers ?? {}), prod };
}

/** `name@version` of a snapshot key, without its peers; undefined for a workspace path. */
export function packageOf(node: string): { name: string; version: string } | undefined {
  const bare = node.split("(")[0] ?? node;
  const at = bare.lastIndexOf("@");
  if (at <= 0) {
    return undefined;
  }
  return { name: bare.slice(0, at), version: bare.slice(at + 1) };
}

/** The snapshots of `name@version`, one per set of peers. */
export function snapshotsOf(graph: Graph, name: string, version: string): string[] {
  const key = `${name}@${version}`;
  return [...graph.prod.keys()].filter((node) => node === key || node.startsWith(`${key}(`));
}
