/**
 * Where each registered ID stands, derived from test results and the evidence
 * of manual checks (plan 1.6); nothing is ticked off by hand.
 *
 * - done: passed in its registered suite, has current evidence, or all of its
 *   sub-IDs are done or settled;
 * - settled: not applicable, or a gap the user confirmed;
 * - pending: nothing ran yet, no evidence yet, a gap, a conditional task's ID,
 *   or sub-IDs still open;
 * - problem: failed, only skipped, ran in another suite only, stale evidence,
 *   or results that should have been a sub-ID's.
 */
import { MILESTONES, type Entry, type Milestone, type Registry } from "./registry.ts";
import type { Result } from "./results.ts";

export type Standing = "done" | "settled" | "pending" | "problem";

export interface State {
  standing: Standing;
  detail: string;
}

export interface Context {
  /** `YYYY-MM-DD`; evidence dated later is wrong, `max_age_days` counts back from it. */
  today: string;
  /** The current version of each component evidence can be bound to. */
  versions: Record<string, string | undefined>;
}

export interface Evaluation {
  states: Map<string, State>;
  /** Results of IDs the registry does not have. */
  stray: Result[];
}

const state = (standing: Standing, detail: string): State => ({ standing, detail });

export function complete(state: State): boolean {
  return state.standing === "done" || state.standing === "settled";
}

export function evaluate(registry: Registry, results: Result[], context: Context): Evaluation {
  const byId = new Map<string, Result[]>();
  for (const result of results) {
    byId.set(result.id, [...(byId.get(result.id) ?? []), result]);
  }
  const states = new Map<string, State>();
  const stateOf = (id: string): State => {
    const entry = registry.entries.get(id);
    let found = states.get(id);
    if (!found && entry) {
      found = judge(entry, byId.get(id) ?? []);
      states.set(id, found);
    }
    return found ?? state("pending", "not registered");
  };

  const judge = (entry: Entry, own: Result[]): State => {
    if (entry.status === "na") {
      return state("settled", `n/a: ${entry.reason}`);
    }
    if (entry.status === "gap-confirmed") {
      return state("settled", `gap, confirmed: ${entry.reason}`);
    }
    let found: State;
    if (entry.parts) {
      const open = entry.parts.filter((part) => !complete(stateOf(part)));
      found = own.length
        ? state("problem", `has results of its own (${sources(own)}); tests carry a sub-ID`)
        : open.length
          ? state("pending", `waiting for ${open.join(", ")}`)
          : state("done", `all ${entry.parts.length} sub-IDs`);
    } else if (entry.suite === "man") {
      found = manual(entry, registry, context);
    } else {
      found = tested(entry, own);
    }
    if (found.standing !== "pending" || entry.status === "active") {
      return found;
    }
    return state("pending", `${entry.status}: ${entry.reason}`);
  };

  for (const id of registry.entries.keys()) {
    stateOf(id);
  }
  const stray = results.filter((result) => !registry.entries.has(result.id));
  return { states, stray };
}

function tested(entry: Entry, own: Result[]): State {
  const failed = own.filter((result) => result.outcome === "fail");
  const passed = own.filter((result) => result.outcome === "pass");
  if (failed.length) {
    return state("problem", `failed: ${sources(failed)}`);
  }
  if (passed.some((result) => result.suite === entry.suite)) {
    return state("done", `passed in ${entry.suite}`);
  }
  if (passed.length) {
    const suites = [...new Set(passed.map((result) => result.suite))].join(", ");
    return state("problem", `ran only in ${suites}, not in its suite ${entry.suite}`);
  }
  if (own.length) {
    return state("problem", `skipped: ${sources(own)}`);
  }
  return state("pending", "no test ran");
}

function manual(entry: Entry, registry: Registry, context: Context): State {
  const [latest] = registry.evidence
    .filter((evidence) => evidence.id === entry.id)
    .sort((a, b) => b.date.localeCompare(a.date));
  if (!latest) {
    return state("pending", "no evidence");
  }
  const at = `evidence of ${latest.date} (${latest.record})`;
  const days = (Date.parse(context.today) - Date.parse(latest.date)) / 86_400_000;
  if (days < 0) {
    return state("problem", `${at} is dated after today`);
  }
  if (entry.maxAgeDays !== undefined && days > entry.maxAgeDays) {
    return state("problem", `${at} is older than ${entry.maxAgeDays} days`);
  }
  for (const [component, precision] of Object.entries(entry.versions ?? {})) {
    const now = context.versions[component];
    const then = latest.versions[component] ?? "?";
    const major = (version: string) => version.replace(/^\D*/, "").split(".")[0];
    if (now === undefined) {
      return state("problem", `${at}: the current ${component} version is unknown`);
    }
    if (precision === "major" ? major(now) !== major(then) : now !== then) {
      return state("problem", `${at} is for ${component} ${then}, now ${now}`);
    }
  }
  return state("done", at);
}

function sources(results: Result[]): string {
  const shown = results.slice(0, 3).map((result) => result.source);
  return results.length > 3 ? `${shown.join("; ")}; ...` : shown.join("; ");
}

/** The entries due by `milestone` (earlier milestones included), or produced by `task`. */
export function select(
  registry: Registry,
  filter: { milestone?: Milestone; task?: string },
): Entry[] {
  const due = (entry: Entry) =>
    filter.milestone === undefined ||
    MILESTONES.indexOf(entry.milestone) <= MILESTONES.indexOf(filter.milestone);
  return [...registry.entries.values()].filter(
    (entry) => due(entry) && (filter.task === undefined || entry.task === filter.task),
  );
}
