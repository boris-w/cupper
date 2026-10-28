/**
 * The app-server methods an adapter uses, summed up from codex's own schema
 * (`codex app-server generate-json-schema`), and how they changed against a
 * baseline (design 5.1.6, ADR-08). No I/O.
 */
import { createHash } from "node:crypto";

/** The schema files that list the methods, by the kind of method each lists. */
export const FILES = {
  request: "ClientRequest.json",
  notification: "ServerNotification.json",
  serverRequest: "ServerRequest.json",
};

/** A JSON Schema node, as far as the summary reads it. */
export interface Node {
  $ref?: string;
  properties?: Record<string, Node>;
  required?: string[];
  enum?: unknown[];
  oneOf?: Node[];
  anyOf?: Node[];
  allOf?: Node[];
  definitions?: Record<string, Node>;
}

/** Each of `FILES` by its name, parsed. */
export type Schemas = Record<string, Node>;

/** `params`: the hash of the params schema; `defs`: the definitions it reaches. */
export interface Method {
  kind: string;
  stable: boolean;
  params: string;
  defs: string[];
}
/** `facets`: the property names, required ones and enum values, which tell a change. */
export interface Def {
  hash: string;
  facets: string[];
}
export interface Summary {
  methods: Record<string, Method>;
  defs: Record<string, Def>;
}

/** `defs`: each definition that changed, `<name> added|removed|changed` and the facets it gained (+) and lost (-). */
export interface Change {
  method: string;
  /** `new`: the adapter uses a method the baseline does not have. */
  change: "gone" | "experimental" | "params" | "new";
  defs?: string[];
}

/** Sorted keys and no descriptions: the same schema always hashes the same. */
function canonical(_key: string, value: unknown) {
  if (!value || typeof value !== "object" || Array.isArray(value)) return value;
  return Object.fromEntries(
    Object.entries(value)
      .filter(([key]) => key !== "description")
      .sort(),
  );
}
const hash = (node: unknown) =>
  createHash("sha256").update(JSON.stringify(node, canonical)).digest("hex").slice(0, 16);

type Found = Map<string, { kind: string; params: Node | null; defs: Record<string, Node> }>;

function methods(schemas: Schemas) {
  const found: Found = new Map();
  for (const [kind, file] of Object.entries(FILES)) {
    const { oneOf = [], definitions: defs = {} } = schemas[file] ?? {};
    for (const { properties: { method, params = null } = {} } of oneOf) {
      if (typeof method?.enum?.[0] === "string") found.set(method.enum[0], { kind, params, defs });
    }
  }
  return found;
}

const REF = "#/definitions/";

/** The definitions `root` reaches through `$ref`s, sorted. */
function closure(defs: Record<string, Node>, root: Node | null): string[] {
  const seen = new Set<string>();
  const walk = (node: unknown) =>
    JSON.stringify(node, (key, value: unknown) => {
      const name = key === "$ref" && typeof value === "string" ? value.replace(REF, "") : "";
      if (!seen.has(name) && defs[name]) {
        seen.add(name);
        walk(defs[name]);
      }
      return value;
    });
  walk(root);
  return [...seen].sort();
}

function facets(def: Node): string[] {
  const found = new Set<string>();
  for (const part of [def, ...(def.oneOf ?? []), ...(def.anyOf ?? []), ...(def.allOf ?? [])]) {
    // A variant of a tagged union goes by its tag: `imageGeneration: property result`.
    const tag = Object.values(part.properties ?? {}).find(({ enum: e }) => e?.length === 1)?.enum;
    const variant = part !== def && tag ? `${String(tag[0])}: ` : "";
    for (const [name, property] of Object.entries(part.properties ?? {})) {
      found.add(`${variant}property ${name}`);
      for (const value of property.enum ?? []) found.add(`${name} = ${String(value)}`);
    }
    for (const name of part.required ?? []) found.add(`${variant}required ${name}`);
    for (const value of part.enum ?? []) found.add(`enum ${String(value)}`);
  }
  return [...found].sort();
}

/**
 * The methods of `exp` (the schema made with `--experimental`) that
 * `adapter`'s source names as a string literal, each stable when `stable`
 * (the schema made without it) has it too.
 */
export function summarize(exp: Schemas, stable: Schemas, adapter: string): Summary {
  const inStable = methods(stable);
  const summary: Summary = { methods: {}, defs: {} };
  for (const [name, { kind, params, defs }] of [...methods(exp)].sort(byName)) {
    if (!adapter.includes(JSON.stringify(name))) continue;
    const reached = closure(defs, params);
    summary.methods[name] = {
      kind,
      stable: inStable.has(name),
      params: hash(params),
      defs: reached,
    };
    for (const def of reached) {
      summary.defs[def] = { hash: hash(defs[def]), facets: facets(defs[def] ?? {}) };
    }
  }
  summary.defs = Object.fromEntries(Object.entries(summary.defs).sort(byName));
  return summary;
}

const byName = ([a]: [string, unknown], [b]: [string, unknown]) => (a < b ? -1 : 1);

function changed(def: string, was: Def | undefined, is: Def | undefined): string[] {
  if (was?.hash === is?.hash) return [];
  const before = new Set(was?.facets);
  const after = new Set(is?.facets);
  const facets = [
    ...[...after].filter((facet) => !before.has(facet)).map((facet) => `+${facet}`),
    ...[...before].filter((facet) => !after.has(facet)).map((facet) => `-${facet}`),
  ];
  return [[def, !was ? "added" : !is ? "removed" : "changed", ...facets].join(" ")];
}

/** Every method of the baseline gone, no longer stable or with other params; every one it lacks. */
export function compare(base: Summary, now: Summary): Change[] {
  const changes: Change[] = [];
  for (const [method, was] of Object.entries(base.methods)) {
    const is = now.methods[method];
    if (!is) {
      changes.push({ method, change: "gone" });
      continue;
    }
    if (was.stable && !is.stable) changes.push({ method, change: "experimental" });
    const defs = [...new Set([...was.defs, ...is.defs])]
      .sort()
      .flatMap((def) =>
        changed(
          def,
          was.defs.includes(def) ? base.defs[def] : undefined,
          is.defs.includes(def) ? now.defs[def] : undefined,
        ),
      );
    if (was.params !== is.params || defs.length) changes.push({ method, change: "params", defs });
  }
  for (const method of Object.keys(now.methods)) {
    if (!base.methods[method]) changes.push({ method, change: "new" });
  }
  return changes;
}

const TOLD = {
  gone: "gone",
  experimental: "experimental only",
  params: "params changed",
  new: "not in the baseline",
};

/** The changes as the report lists them: a line per method, an indented one per definition. */
export const render = (changes: Change[]) =>
  changes.flatMap(({ method, change, defs = [] }) => [
    `${method}: ${TOLD[change]}`,
    ...defs.map((def) => `  ${def}`),
  ]);

/** A drift someone reviewed: a newer codex's against `baseline`, as `render` gave it. */
export interface Review {
  baseline: string;
  reviewed: string;
  task: string;
  verdict: string;
  drift: string[];
}

/** Whether `review` holds this very drift against this very baseline. */
export const reviewed = (drift: string[], baseline: string, review?: Review): review is Review =>
  review?.baseline === baseline && JSON.stringify(review.drift) === JSON.stringify(drift);
