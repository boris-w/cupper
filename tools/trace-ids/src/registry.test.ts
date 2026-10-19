import { describe, expect, test } from "vitest";

import { checkRegistry, parseRegistry } from "./registry.ts";

const VALID = `
[T021]
milestone = "M1a"
IT-sys-peer-current = "pr"
IT-sys-peer-macos14 = { suite = "nightly", status = "gap", reason = "macOS 14" }
IT-sys-peer = { parts = ["IT-sys-peer-current", "IT-sys-peer-macos14"] }

[T077]
milestone = "M1a"
MAN-keyboard = { suite = "man", versions = { electron = "major" }, max_age_days = 90 }

[unplanned]
MAN-tcc = { suite = "man", milestone = "M2" }

[not-ids]
UI-0 = "a PRD requirement"

[[evidence]]
id = "MAN-keyboard"
date = "2026-11-03"
record = "docs/tasks/T077-input/keyboard.md"
versions = { electron = "44.9.0" }
`;

const PLAN = "| T021 | ... | T077 | IT-sys-peer-current MAN-keyboard";

function problemsOf(text: string): string[] {
  return parseRegistry(text).problems;
}

function checked(text: string, docs: Record<string, string> = {}): string[] {
  const { registry, problems } = parseRegistry(text);
  expect(problems).toEqual([]);
  const files = new Map(Object.entries({ "docs/03-plan.md": PLAN, ...docs }));
  return checkRegistry(registry, files, (path) => path === "docs/tasks/T077-input/keyboard.md");
}

describe("UNIT-trace-ids: registry", () => {
  test("reads tasks, IDs, sub-IDs, statuses, unplanned IDs and evidence", () => {
    const { registry, problems } = parseRegistry(VALID);
    expect(problems).toEqual([]);
    expect(registry.entries.get("IT-sys-peer-current")).toEqual({
      id: "IT-sys-peer-current",
      task: "T021",
      milestone: "M1a",
      suite: "pr",
      status: "active",
    });
    expect(registry.entries.get("IT-sys-peer-macos14")).toMatchObject({
      suite: "nightly",
      status: "gap",
      reason: "macOS 14",
    });
    expect(registry.entries.get("IT-sys-peer")?.parts).toHaveLength(2);
    expect(registry.entries.get("MAN-keyboard")).toMatchObject({
      versions: { electron: "major" },
      maxAgeDays: 90,
    });
    expect(registry.entries.get("MAN-tcc")).toMatchObject({ task: undefined, milestone: "M2" });
    expect(registry.notIds.get("UI-0")).toBe("a PRD requirement");
    expect(registry.evidence).toEqual([
      {
        id: "MAN-keyboard",
        date: "2026-11-03",
        record: "docs/tasks/T077-input/keyboard.md",
        versions: { electron: "44.9.0" },
      },
    ]);
    expect(
      checked(VALID, { "docs/design/09-testing.md": "MAN-keyboard, UI-0, IT-sys-peer" }),
    ).toEqual([]);
  });

  test("reports what does not fit the format", () => {
    const bad = (body: string) => problemsOf(`[T001]\nmilestone = "M1a"\n${body}`);
    expect(problemsOf("[T001\n")[0]).toMatch(/^not valid TOML \(line 1\)/);
    expect(problemsOf('[tasks]\nUNIT-a = "pr"')).toEqual([
      "[tasks] is neither a task (Txxx), [unplanned], [not-ids] nor [[evidence]]",
    ]);
    expect(problemsOf('[T001]\nmilestone = "M9"\nUNIT-a = "pr"')).toEqual([
      "[T001]: milestone must be one of M1a, M1b, release, M2, later",
      "UNIT-a in [T001]: no milestone; give the table or the ID one",
    ]);
    expect(bad('unit-a = "pr"')).toEqual(["unit-a in [T001]: not a test ID"]);
    expect(bad('UNIT-a = "weekly"')).toEqual([
      "UNIT-a in [T001]: suite must be one of pr, nightly, release, compat, man",
    ]);
    expect(bad('UNIT-a = "man"\nMAN-a = "pr"')).toEqual([
      "UNIT-a in [T001]: MAN- IDs, and only they, are in the `man` suite",
      "MAN-a in [T001]: MAN- IDs, and only they, are in the `man` suite",
    ]);
    expect(bad('UNIT-a = { suite = "pr", status = "na" }')).toEqual([
      "UNIT-a in [T001]: status na needs a reason",
    ]);
    expect(bad('UNIT-a = { suite = "pr", status = "done", owner = "me" }')).toEqual([
      "UNIT-a in [T001]: unknown field `owner`",
      "UNIT-a in [T001]: status must be one of active, conditional, na, gap, gap-confirmed",
    ]);
    expect(bad('UNIT-a = { suite = "pr", parts = ["UNIT-a-x", "UNIT-a-y"] }')).toEqual([
      "UNIT-a in [T001]: an ID made of sub-IDs has no suite of its own",
    ]);
    expect(bad('UNIT-a = { parts = ["UNIT-a-x"] }')).toEqual([
      "UNIT-a in [T001]: parts must list at least two sub-IDs",
    ]);
    expect(
      bad('UNIT-a = { suite = "pr", max_age_days = 30, versions = { electron = "major" } }'),
    ).toEqual([
      'UNIT-a in [T001]: versions: MAN only, { electron = "major" | "exact" }',
      "UNIT-a in [T001]: max_age_days: MAN only, a whole number of days",
    ]);
    expect(
      bad('MAN-a = { suite = "man", versions = { node = "major" }, max_age_days = 0 }'),
    ).toHaveLength(2);
    expect(problemsOf('[T001]\nmilestone = "M1a"\nUNIT-a = "pr"\n[T002]\nUNIT-a = "pr"')).toEqual([
      "UNIT-a in [T002]: already registered under [T001]",
    ]);
    expect(problemsOf('[not-ids]\nUI-0 = ""')).toHaveLength(1);
    for (const evidence of [
      'id = "MAN-a"\ndate = 2026-11-03\nrecord = "docs/a.md"',
      'id = "MAN-a"\ndate = "2026-02-30"\nrecord = "docs/a.md"',
      'id = "MAN-a"\ndate = "2026-11-03"\nrecord = "docs/a.md"\nby = "me"',
    ]) {
      expect(problemsOf(`[[evidence]]\n${evidence}`), evidence).toEqual([
        '[[evidence]] #1: needs id, date ("YYYY-MM-DD"), record and optional versions only',
      ]);
    }
  });

  test("reports doc IDs that are not registered, and registrations that do not add up", () => {
    expect(
      checked(VALID, { "docs/design/x.md": "PROP-new and UI-0, IT-sys-peer-current" }),
    ).toEqual([
      "PROP-new (docs/design/x.md) is not registered; register it, or list it in [not-ids]",
    ]);
    expect(checked(`${VALID}\n[T099]\nmilestone = "M1a"\nUNIT-x = "pr"\n`)).toEqual([
      "[T099] is not a task of docs/03-plan.md",
    ]);
    expect(checked(VALID.replace('UI-0 = "a', 'IT-sys-peer-current = "a'))).toEqual([
      "IT-sys-peer-current is both registered and in [not-ids]",
    ]);
    const parts = (spec: string) =>
      checked(`[T021]\nmilestone = "M1a"\n${spec}`, { "docs/03-plan.md": "T021" });
    expect(parts('IT-a-x = "pr"\nIT-b = { parts = ["IT-a-x", "IT-b-y"] }')).toEqual([
      "IT-a-x, a sub-ID of IT-b: must start with IT-b-",
      "IT-b-y, a sub-ID of IT-b: not registered",
    ]);
    expect(
      parts(
        'IT-a-x = { suite = "pr", milestone = "M1b" }\nIT-a-y = "pr"\nIT-a-z = "pr"\n' +
          'IT-a = { parts = ["IT-a-x", "IT-a-y"] }',
      ),
    ).toEqual([
      "IT-a-x, a sub-ID of IT-a: due in M1b, after its parent (M1a)",
      "IT-a-z: looks like a sub-ID of IT-a, which does not list it",
    ]);
  });

  test("reports evidence for unknown IDs, missing records and missing versions", () => {
    const evidence = VALID.replace('versions = { electron = "44.9.0" }', "")
      .replace('"docs/tasks/T077-input/keyboard.md"', '"notes/keyboard.md"')
      .concat(
        '[[evidence]]\nid = "MAN-ime"\ndate = "2026-11-04"\nrecord = "docs/tasks/T077-input/keyboard.md"\n',
      );
    expect(checked(evidence)).toEqual([
      "evidence for MAN-keyboard: the record notes/keyboard.md is not a file under docs/",
      "evidence for MAN-keyboard: names no electron version",
      "evidence for MAN-ime: not a registered MAN- ID",
    ]);
  });
});
