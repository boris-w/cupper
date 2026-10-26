/**
 * The checks of one JS entry (ADR-11, design 5.1.6): no package of
 * Anthropic's in its production closure, and every package it bundles under
 * a license of the allowlist.
 */
import type { Bundled } from "./bundle.ts";
import { packageOf, snapshotsOf, type Graph } from "./lockfile.ts";

/** Never in the app: Anthropic's SDKs and Claude Code (`@anthropic-ai/claude-code`). */
export const forbidden = (name: string) => name.startsWith("@anthropic-ai/");

export interface Entry {
  name: string;
  /** The workspace package the bundle is built from, which bundles even its devDependencies. */
  root: string;
  bundled: Bundled;
}

/**
 * The production closure: what the entry bundles, and from there every
 * production dependency the lockfile records, bundled or not, so that a
 * dependency the bundler shook out still counts. Each node maps to the node
 * it was reached from; what the entry bundles, to the entry's root.
 */
export function closure(entry: Entry, graph: Graph): Map<string, string> {
  const parent = new Map([...entry.bundled.importers].map((node) => [node, entry.root]));
  const grow = () => {
    for (const node of parent.keys()) {
      for (const next of graph.prod.get(node) ?? []) {
        if (!parent.has(next)) parent.set(next, node);
      }
    }
  };
  // From the workspace first, so that a path shows which of its packages brings a dependency in.
  grow();
  for (const p of entry.bundled.packages.values()) {
    for (const node of snapshotsOf(graph, p.name, p.version)) {
      if (!parent.has(node)) parent.set(node, entry.root);
    }
  }
  grow();
  return parent;
}

/** How the entry's root reaches `target` in the closure, as names. */
function path(entry: Entry, parent: Map<string, string>, target: string): string[] {
  const steps = [target];
  for (let at = target; at !== entry.root;) {
    at = parent.get(at) ?? entry.root;
    steps.unshift(at);
  }
  return steps.map((node) => packageOf(node)?.name ?? node);
}

/** Whether an SPDX expression's licenses are allowed: any side of `OR`, both of `AND`. */
export function allowedLicense(expression: string, allow: Set<string>): boolean {
  const text = expression.trim().replace(/^\((.*)\)$/, "$1");
  const either = text.split(/\s+OR\s+/);
  if (either.length > 1) {
    return either.some((part) => allowedLicense(part, allow));
  }
  const both = text.split(/\s+AND\s+/);
  if (both.length > 1) {
    return both.every((part) => allowedLicense(part, allow));
  }
  return allow.has(text);
}

/** The entry's problems: forbidden packages with their path, and licenses outside the allowlist. */
export function checkEntry(
  entry: Entry,
  graph: Graph,
  license: (dir: string) => string | undefined,
  allow: Set<string>,
): string[] {
  const parent = closure(entry, graph);
  // A bundled package the lockfile does not know still counts.
  for (const p of entry.bundled.packages.values()) {
    if (snapshotsOf(graph, p.name, p.version).length === 0) {
      parent.set(`${p.name}@${p.version}`, entry.root);
    }
  }
  const problems: string[] = [];
  for (const node of [...parent.keys()].filter((node) => forbidden(packageOf(node)?.name ?? ""))) {
    problems.push(`${entry.name}: ${path(entry, parent, node).join(" → ")} (ADR-11)`);
  }
  for (const p of entry.bundled.packages.values()) {
    const text = license(p.dir);
    if (text === undefined || !allowedLicense(text, allow)) {
      problems.push(
        `${entry.name}: ${p.name}@${p.version} bundles under ${text ?? "no license"}, not in the allowlist`,
      );
    }
  }
  return problems;
}
