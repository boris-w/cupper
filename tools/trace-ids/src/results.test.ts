import { describe, expect, test } from "vitest";

import { readJunit, readPlaywright, readSummary } from "./results.ts";

describe("UNIT-trace-ids: results", () => {
  test("come from the per-ID list of an xtask ci summary", () => {
    const summary = JSON.stringify({
      version: 1,
      commit: "0123456789abcdef0123456789abcdef01234567",
      target: "pr",
      jobs: [],
      results: [
        { id: "UNIT-ci-lock", suite: "pr", result: "pass", commit: "0123" },
        { id: "UNIT-ci-summary", suite: "pr", result: "fail", commit: "0123" },
      ],
    });
    expect(readSummary(summary, "pr.json")).toEqual({
      commit: "0123456789abcdef0123456789abcdef01234567",
      results: [
        { id: "UNIT-ci-lock", suite: "pr", outcome: "pass", source: "pr.json" },
        { id: "UNIT-ci-summary", suite: "pr", outcome: "fail", source: "pr.json" },
      ],
    });
    expect(() => readSummary('{"version": 2, "commit": "x", "results": []}', "a.json")).toThrow(
      "a.json: not a summary",
    );
    const unknown = '{"version": 1, "commit": "x", "results": [{"id": "A", "suite": "pr"}]}';
    expect(() => readSummary(unknown, "b.json")).toThrow("b.json: a result needs");
  });

  test("come from the test cases of a cargo-nextest JUnit report", () => {
    // Shaped like nextest's report: a failure with its output, a skipped and
    // a flaky case, a case with no ID, escaped names and a CDATA section.
    const xml = `<?xml version="1.0" encoding="UTF-8"?>
<testsuites name="nextest-run" tests="6" failures="1" errors="0" time="1.234">
    <testsuite name="xtask" tests="4" disabled="1" errors="0" failures="1">
        <testcase name="ci::tests::unit_ci_lock_refuses_dirty_tree" classname="xtask" time="0.012">
        </testcase>
        <testcase name="ci::tests::unit_ci_lock_waits" classname="xtask" time="0.003">
            <failure type="test failure">thread &apos;ci::tests::unit_ci_lock_waits&apos; panicked</failure>
            <system-out><![CDATA[<testcase name="unit_ci_summary_fake"/>]]></system-out>
        </testcase>
        <!-- <testcase name="unit_ci_summary_commented"/> -->
        <testcase name="ci::tests::unit_ci_summary_ignored" classname="xtask" time="0.000">
            <skipped/>
        </testcase>
        <testcase name="ci::tests::helper" classname="xtask" time="0.001"/>
    </testsuite>
    <testsuite name="cupper-host::migrations" tests="2" errors="0" failures="0">
        <testcase name="it_migrations_flaky" classname="cupper-host::migrations" time="0.5">
            <flakyFailure type="test failure" message="timed out">first attempt</flakyFailure>
        </testcase>
        <testcase name='it_migrations_&quot;quoted&quot;_&#x41;' classname="cupper-host::migrations"/>
    </testsuite>
</testsuites>`;
    const ids = ["UNIT-ci-lock", "UNIT-ci-summary", "IT-migrations"];
    const results = readJunit(xml, "pr", ids, "junit.xml");
    expect(results.map(({ id, outcome, source }) => [id, outcome, source])).toEqual([
      ["UNIT-ci-lock", "pass", "junit.xml: ci::tests::unit_ci_lock_refuses_dirty_tree"],
      ["UNIT-ci-lock", "fail", "junit.xml: ci::tests::unit_ci_lock_waits"],
      ["UNIT-ci-summary", "skip", "junit.xml: ci::tests::unit_ci_summary_ignored"],
      ["IT-migrations", "fail", "junit.xml: it_migrations_flaky"],
      ["IT-migrations", "pass", 'junit.xml: it_migrations_"quoted"_A'],
    ]);
    expect(results.every((result) => result.suite === "pr")).toBe(true);
  });

  test("come from the tests of a Playwright JSON report, one per project", () => {
    const spec = (title: string, ...statuses: string[]) => ({
      title,
      ok: true,
      tests: statuses.map((status) => ({ projectName: "electron", status, results: [] })),
    });
    const report = JSON.stringify({
      config: {},
      suites: [
        {
          title: "nfr1.spec.ts",
          file: "nfr1.spec.ts",
          specs: [],
          suites: [{ title: "E2E-NFR1-1", specs: [spec("views match @smoke", "expected")] }],
        },
        {
          title: "sec.spec.ts",
          file: "sec.spec.ts",
          specs: [
            spec("E2E-SEC-csp blocks injected scripts", "flaky"),
            spec("E2E-SEC-egress samples nettop", "skipped"),
            spec("E2E-SEC-fake-host", "unexpected", "expected"),
            spec("helper", "expected"),
          ],
        },
      ],
      errors: [],
    });
    const ids = ["E2E-NFR1-1", "E2E-SEC-csp", "E2E-SEC-egress", "E2E-SEC-fake-host"];
    const results = readPlaywright(report, "nightly", ids, "e2e.json");
    expect(results.map(({ id, outcome }) => [id, outcome])).toEqual([
      ["E2E-NFR1-1", "pass"],
      ["E2E-SEC-csp", "fail"],
      ["E2E-SEC-egress", "skip"],
      ["E2E-SEC-fake-host", "fail"],
      ["E2E-SEC-fake-host", "pass"],
    ]);
    expect(results[0]).toEqual({
      id: "E2E-NFR1-1",
      suite: "nightly",
      outcome: "pass",
      source: "e2e.json: nfr1.spec.ts::E2E-NFR1-1::views match @smoke",
    });
    expect(() => readPlaywright("{}", "pr", ids, "x.json")).toThrow("not a Playwright JSON report");
  });
});
