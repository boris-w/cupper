/**
 * The protocol's direction-aware compatibility check (design 5.12.5): the
 * exported schema against a released one. A definition travels from the
 * client to the host (`in`: params, client notifications), from the host to
 * the client (`out`: results, host notifications, the event envelope and
 * body, the views, the error object), or both, which takes both rules.
 *
 * Breaking: a method, a definition or a field removed; a method's scope or
 * idempotency changed; a type changed; `out`, a required field made
 * optional, or a value or branch added where nothing unknown is accepted;
 * `in`, a field added as required or made required, or a value or branch
 * removed. Everything else is additive: methods, optional fields, values of
 * open enums and branches of open unions, such as a new kind of event.
 */

export type Dir = "in" | "out";
export type Node = Record<string, unknown>;

export interface Doc {
  $defs: Record<string, Node>;
  "x-methods": Record<string, Record<string, Node>>;
}

export interface Finding {
  at: string;
  change: string;
  breaking: boolean;
}

/** Exported next to the method catalog (design 5.3.10); all go to the client. */
const EXPORTED = ["Event", "EventBody", "IndexView", "SessionView", "RpcError"];
const BOTH: ReadonlySet<Dir> = new Set(["in", "out"]);

const isNode = (value: unknown): value is Node =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const nodes = (value: unknown): Node[] => (Array.isArray(value) ? value.filter(isNode) : []);
const list = (value: unknown): unknown[] => (Array.isArray(value) ? value : []);
const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const refName = (ref: unknown) => String(ref).replace("#/$defs/", "");

/** The directions each definition travels in, by what refers to it. */
export function directions(doc: Doc): Map<string, Set<Dir>> {
  const found = new Map<string, Set<Dir>>();
  const mark = (value: unknown, dir: Dir): void => {
    if (Array.isArray(value)) return value.forEach((item) => mark(item, dir));
    if (!isNode(value)) return;
    if (typeof value.$ref === "string") {
      const name = refName(value.$ref);
      const dirs = found.get(name) ?? new Set<Dir>();
      if (dirs.has(dir)) return;
      found.set(name, dirs.add(dir));
      mark(doc.$defs[name], dir);
    }
    Object.values(value).forEach((child) => mark(child, dir));
  };
  for (const [table, methods] of Object.entries(doc["x-methods"])) {
    for (const method of Object.values(methods)) {
      mark(method.params, table === "host_notifications" ? "out" : "in");
      mark(method.result, "out");
    }
  }
  EXPORTED.forEach((name) => mark({ $ref: `#/$defs/${name}` }, "out"));
  return found;
}

export function compare(released: Doc, current: Doc): Finding[] {
  const findings: Finding[] = [];
  const report = (at: string, change: string, breaking: boolean) =>
    void findings.push({ at, change, breaking });

  /** A union branch's identity: its definition, its tag's value, or its kind. */
  const key = (branch: Node): string => {
    if (typeof branch.$ref === "string") return refName(branch.$ref);
    const tag = Object.entries(isNode(branch.properties) ? branch.properties : {}).find(
      ([, property]) => isNode(property) && "const" in property,
    );
    if (tag) return `${tag[0]}=${String((tag[1] as Node).const)}`;
    return branch.enum ? "enum" : JSON.stringify(branch.type ?? branch);
  };
  /** Whether a union takes what it does not know: any string, or an `Unknown…` member. */
  const open = (branches: Node[]) =>
    branches.some(
      (b) =>
        refName(b.$ref ?? "").startsWith("Unknown") ||
        (b.type === "string" && !("enum" in b) && !("const" in b)),
    );

  const values = (
    at: string,
    was: unknown[],
    now: unknown[],
    dirs: ReadonlySet<Dir>,
    opened: boolean,
  ) => {
    for (const value of was.filter((v) => !now.some((n) => same(n, v)))) {
      report(`${at} ${JSON.stringify(value)}`, "value removed", dirs.has("in"));
    }
    for (const value of now.filter((v) => !was.some((w) => same(w, v)))) {
      report(`${at} ${JSON.stringify(value)}`, "value added", !opened && dirs.has("out"));
    }
  };

  const union = (at: string, was: Node[], now: Node[], dirs: ReadonlySet<Dir>) => {
    const after = new Map(now.map((branch) => [key(branch), branch]));
    const before = new Map(was.map((branch) => [key(branch), branch]));
    for (const [name, branch] of before) {
      const next = after.get(name);
      if (!next) report(`${at} | ${name}`, "branch removed", dirs.has("in"));
      else if (name === "enum") values(at, list(branch.enum), list(next.enum), dirs, open(was));
      else node(`${at} | ${name}`, branch, next, dirs);
    }
    for (const name of after.keys()) {
      if (!before.has(name))
        report(`${at} | ${name}`, "branch added", !open(was) && dirs.has("out"));
    }
  };

  const object = (at: string, was: Node, now: Node, dirs: ReadonlySet<Dir>) => {
    const props = (n: Node) => (isNode(n.properties) ? n.properties : {});
    const required = (n: Node) => new Set(Array.isArray(n.required) ? n.required : []);
    const [before, after, wasRequired, isRequired] = [
      props(was),
      props(now),
      required(was),
      required(now),
    ];
    for (const [name, property] of Object.entries(before)) {
      const field = `${at}.${name}`;
      if (!(name in after)) {
        report(field, "field removed", true);
        continue;
      }
      if (wasRequired.has(name) && !isRequired.has(name)) {
        report(field, "required made optional", dirs.has("out"));
      }
      if (!wasRequired.has(name) && isRequired.has(name)) {
        report(field, "optional made required", dirs.has("in"));
      }
      node(field, property, after[name], dirs);
    }
    for (const name of Object.keys(after).filter((n) => !(n in before))) {
      const required = isRequired.has(name) && dirs.has("in");
      report(`${at}.${name}`, required ? "field added, required" : "field added", required);
    }
  };

  function node(at: string, was: unknown, now: unknown, dirs: ReadonlySet<Dir>): void {
    if (!isNode(was) || !isNode(now)) {
      if (!same(was, now)) report(at, now === undefined ? "removed" : "changed", true);
      return;
    }
    if ("$ref" in was || "$ref" in now) {
      const name = (n: Node) => ("$ref" in n ? refName(n.$ref) : "an inline schema");
      if (was.$ref !== now.$ref) report(at, `type ${name(was)} became ${name(now)}`, true);
      return;
    }
    if (!same(was.type, now.type))
      report(at, `type ${String(was.type)} became ${String(now.type)}`, true);
    if (!same(was.const, now.const)) report(at, "constant changed", true);
    if ("enum" in was || "enum" in now) values(at, list(was.enum), list(now.enum), dirs, false);
    for (const kind of ["anyOf", "oneOf"]) {
      if (kind in was || kind in now) {
        union(
          at,
          kind in was ? nodes(was[kind]) : [was],
          kind in now ? nodes(now[kind]) : [now],
          dirs,
        );
      }
    }
    if ("allOf" in was || "allOf" in now) {
      const [a, b] = [nodes(was.allOf), nodes(now.allOf)];
      if (a.length !== b.length) report(at, "allOf changed", true);
      else a.forEach((part, n) => node(`${at} & ${n}`, part, b[n], dirs));
    }
    if ("properties" in was || "properties" in now) object(at, was, now, dirs);
    for (const child of ["items", "additionalProperties"]) {
      if (isNode(was[child]) || isNode(now[child]))
        node(`${at}.<${child}>`, was[child], now[child], dirs);
    }
  }

  for (const [table, methods] of Object.entries(released["x-methods"])) {
    for (const [name, method] of Object.entries(methods)) {
      const at = `${table}/${name}`;
      const next = current["x-methods"][table]?.[name];
      if (!next) {
        report(at, "method removed", true);
        continue;
      }
      for (const meta of ["x-scope", "x-idempotent"]) {
        if (method[meta] !== next[meta])
          report(`${at} ${meta}`, `${String(method[meta])} became ${String(next[meta])}`, true);
      }
      const into: ReadonlySet<Dir> = new Set([table === "host_notifications" ? "out" : "in"]);
      node(`${at} params`, method.params, next.params, into);
      if ("result" in method) node(`${at} result`, method.result, next.result, new Set(["out"]));
    }
    for (const name of Object.keys(current["x-methods"][table] ?? {})) {
      if (!(name in methods)) report(`${table}/${name}`, "method added", false);
    }
  }
  const [was, now] = [directions(released), directions(current)];
  for (const [name, definition] of Object.entries(released.$defs)) {
    const dirs = new Set([...(was.get(name) ?? BOTH), ...(now.get(name) ?? [])]);
    if (name in current.$defs) node(`$defs/${name}`, definition, current.$defs[name], dirs);
    else report(`$defs/${name}`, "definition removed", true);
  }
  return findings;
}

/** Where an integer is not bounded within ±(2^53 − 1), or keeps a `format` (design 5.3.1). */
export function unsafeIntegers(value: unknown, at = "#"): string[] {
  if (Array.isArray(value)) return value.flatMap((item, n) => unsafeIntegers(item, `${at}/${n}`));
  if (!isNode(value)) return [];
  const found: string[] = [];
  const type = value.type;
  if (type === "integer" || (Array.isArray(type) && type.includes("integer"))) {
    const { minimum: lo, maximum: hi } = value;
    const safe = (n: unknown) => typeof n === "number" && Math.abs(n) <= Number.MAX_SAFE_INTEGER;
    if (!safe(lo) || !safe(hi) || "format" in value) found.push(at);
  }
  for (const [name, child] of Object.entries(value))
    found.push(...unsafeIntegers(child, `${at}/${name}`));
  return found;
}
