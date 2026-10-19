import { describe, expect, test } from "vitest";

import { complete, evaluate, select, type Context } from "./evaluate.ts";
import { parseRegistry, type Registry } from "./registry.ts";
import type { Outcome, Result } from "./results.ts";

const REGISTRY = `
[T001]
milestone = "M1a"
UNIT-a = "pr"
UNIT-b = "pr"
E2E-c = "nightly"
IT-gap = { suite = "nightly", status = "gap", reason = "macOS 14" }
IT-gap-ok = { suite = "nightly", status = "gap-confirmed", reason = "user, 2026-12-01" }
IT-na = { suite = "pr", status = "na", reason = "K1 passed" }
IT-cond = { suite = "pr", status = "conditional", reason = "K2 fails" }
IT-f-core = "pr"
MAN-keyboard = { suite = "man", versions = { electron = "major" } }
MAN-doc = { suite = "man", max_age_days = 30 }

[T002]
milestone = "M1b"
IT-f-git = "pr"
IT-f = { parts = ["IT-f-core", "IT-f-git"] }
MAN-ui = "man"

[[evidence]]
id = "MAN-keyboard"
date = "2026-10-01"
record = "docs/a.md"
versions = { electron = "44.9.0" }

[[evidence]]
id = "MAN-doc"
date = "2026-10-01"
record = "docs/b.md"
`;

const CONTEXT: Context = { today: "2026-10-19", versions: { electron: "44.10.1" } };

function registry(text = REGISTRY): Registry {
  const parsed = parseRegistry(text);
  expect(parsed.problems).toEqual([]);
  return parsed.registry;
}

function result(id: string, suite: string, outcome: Outcome): Result {
  return { id, suite, outcome, source: `${suite}.json` };
}

/** `[standing, detail]` of each ID, after `results`. */
function states(results: Result[], context = CONTEXT, text = REGISTRY) {
  const { states } = evaluate(registry(text), results, context);
  return Object.fromEntries([...states].map(([id, state]) => [id, [state.standing, state.detail]]));
}

describe("UNIT-trace-ids: evaluate", () => {
  test("an ID is done when it passed in its registered suite", () => {
    const found = states([
      result("UNIT-a", "pr", "pass"),
      result("UNIT-a", "nightly", "pass"),
      result("E2E-c", "nightly", "pass"),
    ]);
    expect(found["UNIT-a"]).toEqual(["done", "passed in pr"]);
    expect(found["E2E-c"]).toEqual(["done", "passed in nightly"]);
  });

  test("reports an ID with no test", () => {
    expect(states([])["UNIT-a"]).toEqual(["pending", "no test ran"]);
  });

  test("reports an ID whose tests were skipped", () => {
    expect(states([result("UNIT-a", "pr", "skip")])["UNIT-a"]).toEqual([
      "problem",
      "skipped: pr.json",
    ]);
    // A skipped test does not count; the ID's other tests do.
    const mixed = [result("UNIT-a", "pr", "skip"), result("UNIT-a", "pr", "pass")];
    expect(states(mixed)["UNIT-a"]).toEqual(["done", "passed in pr"]);
  });

  test("reports an ID that ran in another suite only", () => {
    expect(states([result("UNIT-a", "nightly", "pass")])["UNIT-a"]).toEqual([
      "problem",
      "ran only in nightly, not in its suite pr",
    ]);
    // e2e runs only its smoke subset in pr: passing there does not cover nightly.
    expect(states([result("E2E-c", "pr", "pass")])["E2E-c"]?.[0]).toBe("problem");
  });

  test("reports an ID that failed anywhere", () => {
    const found = states([result("UNIT-b", "pr", "pass"), result("UNIT-b", "nightly", "fail")]);
    expect(found["UNIT-b"]).toEqual(["problem", "failed: nightly.json"]);
  });

  test("reports manual evidence that is missing, stale or from another version", () => {
    const found = states([]);
    expect(found["MAN-ui"]).toEqual(["pending", "no evidence"]);
    expect(found["MAN-keyboard"]).toEqual(["done", "evidence of 2026-10-01 (docs/a.md)"]);
    expect(found["MAN-doc"]).toEqual(["done", "evidence of 2026-10-01 (docs/b.md)"]);

    const upgraded = states([], { today: "2026-10-19", versions: { electron: "45.0.0" } });
    expect(upgraded["MAN-keyboard"]).toEqual([
      "problem",
      "evidence of 2026-10-01 (docs/a.md) is for electron 44.9.0, now 45.0.0",
    ]);
    const later = states([], { today: "2026-11-15", versions: {} });
    expect(later["MAN-doc"]).toEqual([
      "problem",
      "evidence of 2026-10-01 (docs/b.md) is older than 30 days",
    ]);
    expect(later["MAN-keyboard"]).toEqual([
      "problem",
      "evidence of 2026-10-01 (docs/a.md): the current electron version is unknown",
    ]);
    const early = states([], { today: "2026-09-30", versions: { electron: "44.9.0" } });
    expect(early["MAN-keyboard"]?.[1]).toBe(
      "evidence of 2026-10-01 (docs/a.md) is dated after today",
    );

    // The latest evidence counts.
    const renewed = `${REGISTRY}
[[evidence]]
id = "MAN-keyboard"
date = "2026-10-18"
record = "docs/c.md"
versions = { electron = "45.0.1" }
`;
    expect(
      states([], { today: "2026-10-19", versions: { electron: "45.0.0" } }, renewed)[
        "MAN-keyboard"
      ],
    ).toEqual(["done", "evidence of 2026-10-18 (docs/c.md)"]);
  });

  test("reports sub-IDs that are not all done", () => {
    expect(states([result("IT-f-core", "pr", "pass")])["IT-f"]).toEqual([
      "pending",
      "waiting for IT-f-git",
    ]);
    const both = [result("IT-f-core", "pr", "pass"), result("IT-f-git", "pr", "pass")];
    expect(states(both)["IT-f"]).toEqual(["done", "all 2 sub-IDs"]);
    expect(states([...both, result("IT-f", "pr", "pass")])["IT-f"]?.[0]).toBe("problem");
  });

  test("statuses settle an ID or say why it is still open", () => {
    const found = states([result("IT-gap", "nightly", "fail")]);
    expect(found["IT-na"]).toEqual(["settled", "n/a: K1 passed"]);
    expect(found["IT-gap-ok"]).toEqual(["settled", "gap, confirmed: user, 2026-12-01"]);
    expect(found["IT-cond"]).toEqual(["pending", "conditional: K2 fails"]);
    expect(found["IT-gap"]?.[0]).toBe("problem");
    expect(states([])["IT-gap"]).toEqual(["pending", "gap: macOS 14"]);
    expect(states([result("IT-gap", "nightly", "pass")])["IT-gap"]?.[0]).toBe("done");
  });

  test("results of unregistered IDs stray", () => {
    const stray = result("UI-0", "pr", "pass");
    expect(evaluate(registry(), [stray, result("UNIT-a", "pr", "pass")], CONTEXT).stray).toEqual([
      stray,
    ]);
  });

  test("a milestone selects the IDs due by then; a task, its own", () => {
    const all = registry();
    const ids = (entries: { id: string }[]) => entries.map((entry) => entry.id).sort();
    expect(ids(select(all, { milestone: "M1a" }))).toHaveLength(10);
    expect(ids(select(all, { milestone: "M1b" }))).toHaveLength(13);
    expect(ids(select(all, { task: "T002" }))).toEqual(["IT-f", "IT-f-git", "MAN-ui"]);
    expect(complete({ standing: "settled", detail: "" })).toBe(true);
    expect(complete({ standing: "problem", detail: "" })).toBe(false);
  });
});
