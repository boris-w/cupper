/**
 * The test ID registry `tests/ids.toml` (design 5.9.10, plan 1.6); its header
 * describes the format. One table per producing task maps each ID to the suite
 * it must pass in, or to a table of fields (`Entry`).
 */
import { parse, TomlError } from "smol-toml";

import { idsIn, isId } from "./ids.ts";

export const MILESTONES = ["M1a", "M1b", "release", "M2", "later"] as const;
export const SUITES = ["pr", "nightly", "release", "compat", "man"] as const;
export const STATUSES = ["active", "conditional", "na", "gap", "gap-confirmed"] as const;
/** Components a manual check can be bound to: its evidence names their versions. */
export const COMPONENTS = ["electron"] as const;
const PRECISIONS = ["major", "exact"] as const;
const FIELDS = [
  ...["suite", "milestone", "status", "reason", "parts", "versions", "max_age_days"],
  ...["scenario", "cli"],
];
/** The CLIs a compatibility report can be for. */
export const CLIS = ["claude", "codex"] as const;

export type Milestone = (typeof MILESTONES)[number];
export type Suite = (typeof SUITES)[number];
export type Status = (typeof STATUSES)[number];
export type Precision = (typeof PRECISIONS)[number];

export interface Entry {
  id: string;
  /** The producing task; absent for the IDs in `[unplanned]`. */
  task?: string;
  milestone: Milestone;
  /** Absent for an ID made of sub-IDs. */
  suite?: Suite;
  /** The sub-IDs; the ID is done when all of them are. */
  parts?: string[];
  status: Status;
  reason?: string;
  /** MAN only: evidence must name these components at their current version. */
  versions?: Record<string, Precision>;
  /** MAN only: evidence older than this many days is stale. */
  maxAgeDays?: number;
  /** COMPAT only: the compatibility suite's scenario and the CLI that proves the ID. */
  scenario?: string;
  cli?: (typeof CLIS)[number];
}

export interface Evidence {
  id: string;
  /** `YYYY-MM-DD`. */
  date: string;
  /** The record of the check, a file under `docs/`. */
  record: string;
  versions: Record<string, string>;
}

export interface Registry {
  entries: Map<string, Entry>;
  notIds: Map<string, string>;
  evidence: Evidence[];
}

/** Reads the registry; `problems` lists everything that does not fit the format. */
export function parseRegistry(text: string): { registry: Registry; problems: string[] } {
  const registry: Registry = { entries: new Map(), notIds: new Map(), evidence: [] };
  const problems: string[] = [];
  let doc: Record<string, unknown>;
  try {
    doc = parse(text);
  } catch (err) {
    const line = err instanceof TomlError ? ` (line ${err.line})` : "";
    problems.push(`not valid TOML${line}: ${(err as Error).message.split("\n")[0]}`);
    return { registry, problems };
  }
  for (const [name, value] of Object.entries(doc)) {
    if (name === "not-ids") {
      readNotIds(value, registry, problems);
    } else if (name === "evidence") {
      readEvidence(value, registry, problems);
    } else if (name === "unplanned" || /^T\d{3}$/.test(name)) {
      readSection(name, value, registry, problems);
    } else {
      problems.push(`[${name}] is neither a task (Txxx), [unplanned], [not-ids] nor [[evidence]]`);
    }
  }
  const proven = new Map<string, string>();
  for (const { id, cli, scenario } of registry.entries.values()) {
    const by = `${cli} ${scenario}`;
    const earlier = cli && proven.get(by);
    if (earlier) problems.push(`${id}: ${earlier} is already proven by the ${by} scenario`);
    if (cli) proven.set(by, id);
  }
  return { registry, problems };
}

function readSection(name: string, value: unknown, registry: Registry, problems: string[]): void {
  if (!isTable(value)) {
    problems.push(`[${name}] must be a table`);
    return;
  }
  const { milestone, ...ids } = value;
  if (milestone !== undefined && !oneOf(MILESTONES, milestone)) {
    problems.push(`[${name}]: milestone must be one of ${MILESTONES.join(", ")}`);
  }
  for (const [id, spec] of Object.entries(ids)) {
    const at = `${id} in [${name}]`;
    const earlier = registry.entries.get(id);
    if (!isId(id)) {
      problems.push(`${at}: not a test ID`);
      continue;
    }
    if (earlier) {
      problems.push(`${at}: already registered under [${earlier.task ?? "unplanned"}]`);
      continue;
    }
    const entry = readEntry(id, spec, at, problems);
    const due = entry?.milestone ?? (oneOf(MILESTONES, milestone) ? milestone : undefined);
    if (entry && !due) {
      problems.push(`${at}: no milestone; give the table or the ID one`);
    } else if (entry && due) {
      const task = name === "unplanned" ? undefined : name;
      registry.entries.set(id, { ...entry, id, task, milestone: due });
    }
  }
}

type Fields = Omit<Entry, "id" | "task" | "milestone"> & { milestone?: Milestone };

function readEntry(id: string, spec: unknown, at: string, problems: string[]): Fields | undefined {
  const fields = typeof spec === "string" ? { suite: spec } : isTable(spec) ? spec : undefined;
  if (!fields) {
    problems.push(`${at}: give the suite, or a table of fields`);
    return undefined;
  }
  const count = problems.length;
  const fail = (what: string) => problems.push(`${at}: ${what}`);
  for (const key of Object.keys(fields).filter((key) => !FIELDS.includes(key))) {
    fail(`unknown field \`${key}\``);
  }
  const { suite, milestone, status = "active", reason, parts, versions, max_age_days } = fields;
  const { scenario, cli } = fields;
  const compat = id.startsWith("COMPAT-");
  if ((scenario !== undefined || cli !== undefined) && !compat) {
    fail("scenario and cli: COMPAT IDs only");
  } else if (compat && suite === "compat" && (typeof scenario !== "string" || !oneOf(CLIS, cli))) {
    fail(`a COMPAT ID in the compat suite names its scenario and its cli (${CLIS.join(" | ")})`);
  }
  const manual = id.startsWith("MAN-");
  if (milestone !== undefined && !oneOf(MILESTONES, milestone)) {
    fail(`milestone must be one of ${MILESTONES.join(", ")}`);
  }
  if (parts !== undefined) {
    if (!Array.isArray(parts) || parts.length < 2 || !parts.every((p) => isId(String(p)))) {
      fail("parts must list at least two sub-IDs");
    }
    if (suite !== undefined) {
      fail("an ID made of sub-IDs has no suite of its own");
    }
  } else if (!oneOf(SUITES, suite)) {
    fail(`suite must be one of ${SUITES.join(", ")}`);
  } else if (manual !== (suite === "man")) {
    fail("MAN- IDs, and only they, are in the `man` suite");
  }
  if (!oneOf(STATUSES, status)) {
    fail(`status must be one of ${STATUSES.join(", ")}`);
  } else if (status !== "active" && (typeof reason !== "string" || reason === "")) {
    fail(`status ${status} needs a reason`);
  } else if (reason !== undefined && typeof reason !== "string") {
    fail("reason must be text");
  }
  if (versions !== undefined) {
    const known = (entry: [string, unknown]) =>
      oneOf(COMPONENTS, entry[0]) && oneOf(PRECISIONS, entry[1]);
    if (!manual || !isTable(versions) || !Object.entries(versions).every(known)) {
      fail(`versions: MAN only, { ${COMPONENTS.join(" | ")} = "major" | "exact" }`);
    }
  }
  const days = (value: unknown) => manual && Number.isInteger(value) && (value as number) > 0;
  if (max_age_days !== undefined && !days(max_age_days)) {
    fail("max_age_days: MAN only, a whole number of days");
  }
  if (problems.length > count) {
    return undefined;
  }
  return {
    status: status as Status,
    milestone: milestone as Milestone | undefined,
    suite: suite as Suite | undefined,
    parts: parts as string[] | undefined,
    reason: reason as string | undefined,
    versions: versions as Record<string, Precision> | undefined,
    maxAgeDays: max_age_days as number | undefined,
    scenario: scenario as string | undefined,
    cli: cli as Entry["cli"],
  };
}

function readNotIds(value: unknown, registry: Registry, problems: string[]): void {
  for (const [word, reason] of Object.entries(isTable(value) ? value : {})) {
    if (!isId(word) || typeof reason !== "string" || reason === "") {
      problems.push(
        `${word} in [not-ids]: give an ID-shaped word and the reason it is not a test ID`,
      );
    } else {
      registry.notIds.set(word, reason);
    }
  }
}

function readEvidence(value: unknown, registry: Registry, problems: string[]): void {
  const records = Array.isArray(value) ? value : [value];
  for (const [index, record] of records.entries()) {
    const fields = isTable(record) ? record : {};
    const { id, date, record: path, versions = {}, ...rest } = fields;
    const ok =
      typeof id === "string" &&
      typeof date === "string" &&
      isDate(date) &&
      typeof path === "string" &&
      isTable(versions) &&
      Object.values(versions).every((version) => typeof version === "string") &&
      Object.keys(rest).length === 0;
    if (ok) {
      registry.evidence.push({
        id,
        date,
        record: path,
        versions: versions as Record<string, string>,
      });
    } else {
      problems.push(
        `[[evidence]] #${index + 1}: needs id, date ("YYYY-MM-DD"), record and optional versions only`,
      );
    }
  }
}

/**
 * Checks the registry against the docs: every ID-shaped word of `docs` (file
 * name to text, the plan among them) is registered or listed in `[not-ids]`,
 * every task is in the plan, sub-IDs and evidence fit. `exists` tells whether
 * a file of the repository exists.
 */
export function checkRegistry(
  registry: Registry,
  docs: Map<string, string>,
  exists: (path: string) => boolean,
): string[] {
  const problems: string[] = [];
  const { entries, notIds } = registry;
  const unregistered = new Map<string, string>();
  for (const [file, text] of docs) {
    for (const id of idsIn(text)) {
      if (!entries.has(id) && !notIds.has(id) && !unregistered.has(id)) {
        unregistered.set(id, file);
      }
    }
  }
  for (const [id, file] of unregistered) {
    problems.push(`${id} (${file}) is not registered; register it, or list it in [not-ids]`);
  }
  for (const word of notIds.keys()) {
    if (entries.has(word)) {
      problems.push(`${word} is both registered and in [not-ids]`);
    }
  }
  const plan = docs.get("docs/03-plan.md") ?? "";
  for (const task of new Set([...entries.values()].flatMap((entry) => entry.task ?? []))) {
    if (!new RegExp(`(?<![A-Za-z0-9])${task}(?![0-9])`).test(plan)) {
      problems.push(`[${task}] is not a task of docs/03-plan.md`);
    }
  }
  problems.push(...checkParts(entries));
  for (const { id, record, versions } of registry.evidence) {
    const entry = entries.get(id);
    if (entry?.suite !== "man") {
      problems.push(`evidence for ${id}: not a registered MAN- ID`);
    }
    if (!record.startsWith("docs/") || !exists(record)) {
      problems.push(`evidence for ${id}: the record ${record} is not a file under docs/`);
    }
    for (const component of Object.keys(entry?.versions ?? {}).filter((c) => !(c in versions))) {
      problems.push(`evidence for ${id}: names no ${component} version`);
    }
  }
  return problems;
}

function checkParts(entries: Map<string, Entry>): string[] {
  const problems: string[] = [];
  const parentOf = new Map<string, string>();
  const due = (entry: Entry) => MILESTONES.indexOf(entry.milestone);
  for (const parent of entries.values()) {
    for (const id of parent.parts ?? []) {
      const part = entries.get(id);
      const at = `${id}, a sub-ID of ${parent.id}`;
      if (!part) {
        problems.push(`${at}: not registered`);
      } else if (!id.startsWith(`${parent.id}-`)) {
        problems.push(`${at}: must start with ${parent.id}-`);
      } else if (part.parts) {
        problems.push(`${at}: has sub-IDs itself`);
      } else if (parentOf.has(id)) {
        problems.push(`${at}: is a sub-ID of ${parentOf.get(id)} too`);
      } else if (due(part) > due(parent)) {
        problems.push(`${at}: due in ${part.milestone}, after its parent (${parent.milestone})`);
      }
      parentOf.set(id, parent.id);
    }
  }
  for (const entry of entries.values()) {
    const parent = [...entries.values()].find(
      (other) => other.parts && entry.id.startsWith(`${other.id}-`),
    );
    if (parent && !parentOf.has(entry.id)) {
      problems.push(`${entry.id}: looks like a sub-ID of ${parent.id}, which does not list it`);
    }
  }
  return problems;
}

export function isTable(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === "object" && value !== null && !Array.isArray(value) && !(value instanceof Date)
  );
}

export function oneOf<T extends string>(list: readonly T[], value: unknown): value is T {
  return list.includes(value as T);
}

/** A real calendar date written `YYYY-MM-DD`. */
export function isDate(text: string): boolean {
  const date = new Date(`${text}T00:00:00Z`);
  const valid = /^\d{4}-\d{2}-\d{2}$/.test(text) && !Number.isNaN(date.getTime());
  return valid && date.toISOString().startsWith(text);
}
