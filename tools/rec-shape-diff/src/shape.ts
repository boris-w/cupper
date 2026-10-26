/**
 * A recording's shape (design 5.9.5, test #7): what an adapter upgrade may
 * change that the host depends on, and nothing of the text, times or IDs.
 * Six kinds, each a set of strings:
 *
 * - `methods`: each method with its direction, `send` to the adapter or `recv`;
 * - `updates`: the `sessionUpdate` kinds of `session/update`;
 * - `meta`: the key paths inside every `_meta`, with the method and the part
 *   (params, result, error) they are in, arrays as `[]`;
 * - `options`: the `optionId` of each permission option, with its kind;
 * - `stops`: the `stopReason` of prompt results;
 * - `errors`: the error codes, with the method they answer.
 *
 * A response is matched to its request by instance, direction and ID; the
 * recorder's notes, stderr and lines that are not JSON are left out.
 */

export const KINDS = ["methods", "updates", "meta", "options", "stops", "errors"] as const;
export type Kind = (typeof KINDS)[number];
export type Shape = Record<Kind, Set<string>>;

type Node = Record<string, unknown>;
const isNode = (value: unknown): value is Node =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** The key paths below `value`, each prefixed with `at`. */
function paths(value: unknown, at: string, out: Set<string>): void {
  if (Array.isArray(value)) value.forEach((item) => paths(item, `${at}[]`, out));
  if (!isNode(value)) return;
  for (const [key, child] of Object.entries(value)) {
    out.add(`${at}.${key}`);
    paths(child, `${at}.${key}`, out);
  }
}

/** Every `_meta` below `value`: the path to it, and its own key paths. */
function metas(value: unknown, at: string, out: Set<string>): void {
  if (Array.isArray(value)) value.forEach((item) => metas(item, `${at}[]`, out));
  if (!isNode(value)) return;
  for (const [key, child] of Object.entries(value)) {
    if (key === "_meta") paths(child, `${at}._meta`, out);
    else metas(child, `${at}.${key}`, out);
  }
}

/** The shape of a recording in its JSONL text. */
export function shape(text: string): Shape {
  const found = Object.fromEntries(KINDS.map((kind) => [kind, new Set<string>()])) as Shape;
  const asked = new Map<string, string>();
  for (const line of text.split("\n").slice(1)) {
    if (!line.trim()) continue;
    const { dir, instance, msg } = JSON.parse(line) as {
      dir: string;
      instance: number;
      msg?: Node;
    };
    if ((dir !== "send" && dir !== "recv") || !isNode(msg)) continue;
    const other = dir === "send" ? "recv" : "send";
    let method = typeof msg.method === "string" ? msg.method : undefined;
    if (method !== undefined && "id" in msg)
      asked.set(`${instance} ${dir} ${JSON.stringify(msg.id)}`, method);
    method ??= asked.get(`${instance} ${other} ${JSON.stringify(msg.id)}`) ?? "?";
    if (typeof msg.method === "string") found.methods.add(`${dir} ${method}`);
    for (const part of ["params", "result", "error"])
      metas(msg[part], `${method} ${part}`, found.meta);
    const params = isNode(msg.params) ? msg.params : {};
    const update = isNode(params.update) ? params.update.sessionUpdate : undefined;
    if (method === "session/update" && typeof update === "string") found.updates.add(update);
    if (method === "session/request_permission" && Array.isArray(params.options)) {
      for (const option of params.options.filter(isNode)) {
        found.options.add(`${String(option.optionId)} (${String(option.kind)})`);
      }
    }
    const result = isNode(msg.result) ? msg.result : {};
    if (method === "session/prompt" && typeof result.stopReason === "string") {
      found.stops.add(result.stopReason);
    }
    if (isNode(msg.error)) found.errors.add(`${method} ${String(msg.error.code)}`);
  }
  return found;
}

export type Diff = Record<Kind, { added: string[]; removed: string[] }>;

/** What `now` has that `was` has not, and the other way round, by kind. */
export function diff(was: Shape, now: Shape): Diff {
  const minus = (a: Set<string>, b: Set<string>) => [...a].filter((x) => !b.has(x)).sort();
  const entries = KINDS.map((kind) => [
    kind,
    { added: minus(now[kind], was[kind]), removed: minus(was[kind], now[kind]) },
  ]);
  return Object.fromEntries(entries) as Diff;
}

export const empty = (d: Diff) => KINDS.every((k) => !d[k].added.length && !d[k].removed.length);
