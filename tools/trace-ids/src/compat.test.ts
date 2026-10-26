import { describe, expect, test } from "vitest";

import { evaluate } from "./evaluate.ts";
import { parseRegistry } from "./registry.ts";
import { readCompat } from "./results.ts";

const REGISTRY = `
[T066]
milestone = "M1a"
COMPAT-claude-thinking = { suite = "compat", scenario = "thinking", cli = "claude" }
COMPAT-claude-six-sessions = { suite = "compat", scenario = "six-sessions", cli = "claude" }
COMPAT-untrusted-project-claude = { suite = "compat", scenario = "untrusted-project", cli = "claude" }
COMPAT-untrusted-project-codex = { suite = "compat", scenario = "untrusted-project", cli = "codex" }
`;

/** A report of `cargo xtask compat`, only the fields trace-ids reads and a few it ignores. */
const report = (outcomes: Record<string, string>, mode = "real_config", cli = "claude") =>
  JSON.stringify({
    ...{ v: 1, commit: "c0ffee", mode, agent: cli, model: "m", duration_ms: 1 },
    adapter: { name: "claude-agent-acp", version: "0.91.0" },
    cli: { name: cli, version: "2.1.296" },
    scenarios: Object.entries(outcomes).map(([id, outcome]) => ({
      id,
      title: id,
      outcome,
      checks: [],
    })),
  });

function standings(outcomes: Record<string, string>, cli = "claude") {
  const { registry } = parseRegistry(REGISTRY);
  const compat = readCompat(
    report(outcomes, "real_config", cli),
    "r.json",
    registry.entries.values(),
  );
  const { states } = evaluate(registry, compat.results, { today: "2026-10-26", versions: {} });
  const all = [...states].map(([id, state]) => [id, `${state.standing}: ${state.detail}`]);
  return { states: Object.fromEntries(all) as Record<string, string>, unproven: compat.unproven };
}

describe("UNIT-trace-ids-compat", () => {
  test("registry: scenario and cli on COMPAT IDs only, required in the compat suite, one ID a pair", () => {
    expect(parseRegistry(REGISTRY).problems).toEqual([]);
    const problems = (line: string) => parseRegistry(`[T066]\nmilestone = "M1a"\n${line}`).problems;
    expect(problems(`IT-x = { suite = "pr", scenario = "thinking", cli = "claude" }`)).toEqual([
      "IT-x in [T066]: scenario and cli: COMPAT IDs only",
    ]);
    expect(problems(`COMPAT-x = "compat"`)).toEqual([
      "COMPAT-x in [T066]: a COMPAT ID in the compat suite names its scenario and its cli (claude | codex)",
    ]);
    expect(
      problems(`COMPAT-x = { suite = "compat", scenario = "a", cli = "gemini" }`),
    ).toHaveLength(1);
    const twice = `COMPAT-x = { suite = "compat", scenario = "a", cli = "claude" }\nCOMPAT-y = { suite = "compat", scenario = "a", cli = "claude" }`;
    expect(problems(twice)).toEqual([
      "COMPAT-y: COMPAT-x is already proven by the claude a scenario",
    ]);
  });

  test("a passed scenario completes its ID, a failed one does not; inconclusive asks for a rerun", () => {
    const { states } = standings({
      thinking: "pass",
      "six-sessions": "fail",
      "untrusted-project": "inconclusive",
    });
    expect(states["COMPAT-claude-thinking"]).toBe("done: passed in compat");
    expect(states["COMPAT-claude-six-sessions"]).toBe(
      "problem: failed: r.json: claude six-sessions",
    );
    expect(states["COMPAT-untrusted-project-claude"]).toBe(
      "problem: skipped: r.json: claude untrusted-project (inconclusive, run it again: completes nothing)",
    );
    // The same scenario of the other CLI proves the other ID only.
    expect(states["COMPAT-untrusted-project-codex"]).toBe("pending: no test ran");
    expect(
      standings({ "untrusted-project": "pass" }, "codex").states["COMPAT-untrusted-project-codex"],
    ).toBe("done: passed in compat");
  });

  test("not_applicable, observed, real_only and unknown outcomes complete nothing", () => {
    for (const outcome of ["not_applicable", "observed", "real_only", "passed?"]) {
      const { states } = standings({ thinking: outcome });
      expect(states["COMPAT-claude-thinking"]).toMatch(/^problem: skipped: .*completes nothing\)$/);
    }
  });

  test("a fake agent's report is refused; scenarios without an ID are listed apart", () => {
    const { registry } = parseRegistry(REGISTRY);
    const entries = [...registry.entries.values()];
    expect(() => readCompat(report({ thinking: "pass" }, "fake"), "r.json", entries)).toThrow(
      "r.json: a fake agent's report proves no COMPAT ID; only a real CLI's does",
    );
    expect(() => readCompat("{}", "r.json", entries)).toThrow("not a compatibility report");
    const { unproven, states } = standings({ "normal-turn": "pass", approval: "fail" });
    expect(unproven).toEqual(["claude normal-turn: pass", "claude approval: fail"]);
    expect(Object.values(states).filter((s) => s.startsWith("problem"))).toEqual([]);
  });
});
