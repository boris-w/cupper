/**
 * Test results per ID. They come from the summaries that `xtask ci --record`
 * writes (the committed per-ID results, plan 1.8), or from raw test reports:
 * cargo-nextest's JUnit XML and Playwright's JSON. A raw report does not say
 * which suite it ran in, so the caller names it.
 */
import { owner } from "./ids.ts";
import { isTable, type Entry } from "./registry.ts";

export type Outcome = "pass" | "fail" | "skip";

export interface Result {
  id: string;
  suite: string;
  outcome: Outcome;
  /** The report it came from, and the test for a raw report. */
  source: string;
}

export interface Summary {
  commit: string;
  results: Result[];
}

const OUTCOMES: readonly unknown[] = ["pass", "fail", "skip"];

/** The per-ID results of a summary written by `xtask ci --record`. */
export function readSummary(text: string, source: string): Summary {
  const summary: unknown = JSON.parse(text);
  if (!isTable(summary) || summary.version !== 1 || typeof summary.commit !== "string") {
    throw new Error(`${source}: not a summary of \`xtask ci --record\` (version 1)`);
  }
  const results = list(summary.results).map((result) => {
    const { id, suite, result: outcome } = result;
    if (typeof id !== "string" || typeof suite !== "string" || !OUTCOMES.includes(outcome)) {
      throw new Error(`${source}: a result needs an id, a suite and pass, fail or skip`);
    }
    return { id, suite, outcome: outcome as Outcome, source };
  });
  return { commit: summary.commit, results };
}

export interface Compat {
  results: Result[];
  /** Scenarios no COMPAT ID is registered for, as `<cli> <scenario>: <outcome>`. */
  unproven: string[];
}

/**
 * The results of a compatibility report (`testdata/compat/report.schema.json`,
 * design 5.9.5) for the COMPAT IDs registered with its CLI and scenarios.
 * `pass` and `fail` count; any other outcome, `inconclusive` or one this
 * version does not know, is a skip, which completes no ID. A fake agent's
 * report proves nothing and is refused.
 */
export function readCompat(text: string, source: string, entries: Iterable<Entry>): Compat {
  const report: unknown = JSON.parse(text);
  const cli = isTable(report) && isTable(report.cli) ? report.cli.name : undefined;
  if (!isTable(report) || report.v !== 1 || typeof cli !== "string") {
    throw new Error(`${source}: not a compatibility report (v 1)`);
  }
  if (report.mode !== "real_config" && report.mode !== "isolated_config") {
    const what = report.mode === "fake" ? "a fake agent's report" : `mode ${String(report.mode)}`;
    throw new Error(`${source}: ${what} proves no COMPAT ID; only a real CLI's does`);
  }
  const ids = new Map([...entries].filter((e) => e.cli === cli).map((e) => [e.scenario, e.id]));
  const compat: Compat = { results: [], unproven: [] };
  for (const { id: scenario, outcome } of list(report.scenarios)) {
    const [id, at] = [ids.get(String(scenario)), `${cli} ${String(scenario)}`];
    if (id === undefined) {
      compat.unproven.push(`${at}: ${String(outcome)}`);
      continue;
    }
    const counted = outcome === "pass" || outcome === "fail" ? outcome : "skip";
    const why = outcome === "inconclusive" ? "inconclusive, run it again" : String(outcome);
    const detail = counted === "skip" ? ` (${why}: completes nothing)` : "";
    compat.results.push({
      id,
      suite: "compat",
      outcome: counted,
      source: `${source}: ${at}${detail}`,
    });
  }
  return compat;
}

/** Elements that mark a JUnit test case as failed: nextest adds `flakyFailure` when a retry passed. */
const FAILURES = ["failure", "error", "flakyFailure", "rerunFailure"];

/**
 * The test cases of a cargo-nextest JUnit report that belong to one of `ids`.
 * A case is named by its `name` attribute, the test's path in its binary, as
 * libtest names it. A failure element fails it, also one that a retry
 * outlived; `<skipped>` skips it.
 */
export function readJunit(
  xml: string,
  suite: string,
  ids: Iterable<string>,
  source: string,
): Result[] {
  const known = [...ids];
  const results: Result[] = [];
  let name: string | undefined;
  let outcome: Outcome = "pass";
  for (const tag of tags(xml)) {
    if (tag.name === "testcase" && !tag.closing) {
      name = tag.attrs.name ?? "";
      outcome = "pass";
    } else if (name !== undefined && !tag.closing && FAILURES.includes(tag.name)) {
      outcome = "fail";
    } else if (name !== undefined && !tag.closing && tag.name === "skipped" && outcome === "pass") {
      outcome = "skip";
    }
    if (name !== undefined && tag.name === "testcase" && (tag.closing || tag.empty)) {
      const id = owner(known, name);
      if (id) {
        results.push({ id, suite, outcome, source: `${source}: ${name}` });
      }
      name = undefined;
    }
  }
  return results;
}

interface Tag {
  name: string;
  closing: boolean;
  empty: boolean;
  attrs: Record<string, string>;
}

const TOKENS =
  /<!--[\s\S]*?-->|<!\[CDATA\[[\s\S]*?\]\]>|<[?!][^>]*>|<(\/?)([A-Za-z_][\w.:-]*)((?:\s+[^\s=/>]+\s*=\s*(?:"[^"]*"|'[^']*'))*)\s*(\/?)>/g;
const ATTRS = /([^\s=/>]+)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
const ENTITIES: Record<string, string> = { lt: "<", gt: ">", amp: "&", quot: '"', apos: "'" };

/** The element tags of a well-formed XML document; comments, CDATA and text are skipped. */
function* tags(xml: string): Generator<Tag> {
  for (const [, slash, name, attrs, empty] of xml.matchAll(TOKENS)) {
    if (name === undefined) {
      continue;
    }
    const decoded: Record<string, string> = {};
    for (const [, key, double, single] of (attrs ?? "").matchAll(ATTRS)) {
      decoded[key ?? ""] = (double ?? single ?? "").replace(
        /&(#x?[0-9a-fA-F]+|\w+);/g,
        (all, ref) =>
          ref.startsWith("#")
            ? String.fromCodePoint(
                Number(ref.startsWith("#x") ? `0x${ref.slice(2)}` : ref.slice(1)),
              )
            : (ENTITIES[ref] ?? all),
      );
    }
    yield { name, closing: slash === "/", empty: empty === "/", attrs: decoded };
  }
}

/** Playwright's test statuses; `flaky` means the test failed before a retry passed. */
const PLAYWRIGHT: Record<string, Outcome> = {
  expected: "pass",
  skipped: "skip",
  unexpected: "fail",
  flaky: "fail",
};

/**
 * The tests of a Playwright JSON report that belong to one of `ids`, named
 * `<file>::<describe>...::<title>` like the Vitest cases of `xtask ci`. A test
 * runs once per project, and each run counts.
 */
export function readPlaywright(
  text: string,
  suite: string,
  ids: Iterable<string>,
  source: string,
): Result[] {
  const report: unknown = JSON.parse(text);
  if (!isTable(report) || !Array.isArray(report.suites)) {
    throw new Error(`${source}: not a Playwright JSON report`);
  }
  const known = [...ids];
  const results: Result[] = [];
  const walk = (group: Record<string, unknown>, titles: string[]) => {
    const path = [...titles, String(group.title)];
    for (const spec of list(group.specs)) {
      const name = [...path, String(spec.title)].join("::");
      const id = owner(known, name);
      for (const test of list(spec.tests)) {
        const outcome = PLAYWRIGHT[String(test.status)] ?? "fail";
        if (id) {
          results.push({ id, suite, outcome, source: `${source}: ${name}` });
        }
      }
    }
    list(group.suites).forEach((child) => walk(child, path));
  };
  list(report.suites).forEach((group) => walk(group, []));
  return results;
}

/** The tables in `value`, which should be an array of them. */
function list(value: unknown): Record<string, unknown>[] {
  return Array.isArray(value) ? value.filter(isTable) : [];
}
