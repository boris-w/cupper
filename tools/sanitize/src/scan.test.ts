import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, test } from "vitest";

import { main } from "./main.ts";
import { patterns } from "./scan.ts";

/** A temporary tree of files; returns its root. */
function tree(files: Record<string, string>): string {
  const root = mkdtempSync(join(tmpdir(), "sanitize-scan-"));
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(join(root, path, ".."), { recursive: true });
    writeFileSync(join(root, path), text);
  }
  return root;
}

function run(args: string[], cwd: string): { code: number; output: string } {
  const lines: string[] = [];
  const code = main(args, cwd, (line: string) => lines.push(line));
  return { code, output: lines.join("\n") };
}

describe("UNIT-sensitive-list", () => {
  const list =
    "# local words, one per line\n\nAcme Widgets\n  # indented comment\nzeta-gw.internal\n";

  test("a hit fails the scan and names the file, the line and the entry, not the text", () => {
    const root = tree({
      "list.txt": list,
      "testdata/a/rec.jsonl": '{"seq":1}\n{"note":"for ACME widgets only"}\n',
      "testdata/b.jsonl": '{"url":"https://zeta-gw.internal/v1"}\n',
      "docs/tasks/T999-x/ci/pr-0123.json": '{"machine":"generic"}\n',
    });
    const { code, output } = run(
      ["words", "--list", "list.txt", "testdata", "docs/tasks/*/ci/*.json"],
      root,
    );
    expect(code).toBe(1);
    expect(output.split("\n")).toEqual([
      "testdata/a/rec.jsonl:2: entry 1 of the sensitive word list",
      "testdata/b.jsonl:1: entry 2 of the sensitive word list",
      "2 findings in 2 of 3 files",
    ]);
    expect(output.toLowerCase()).not.toContain("acme");
    expect(output).not.toContain("zeta");
  });

  test("comments and blank lines are not entries; a clean tree passes", () => {
    const root = tree({
      "list.txt": list,
      "data/a.json": '{"text":"local words, one per line"}\n',
    });
    expect(run(["words", "--list", "list.txt", "data"], root)).toEqual({
      code: 0,
      output: "clean: 1 files",
    });
  });

  test("the list is required, and so is a path to scan", () => {
    const root = tree({ "data/a.json": "{}\n" });
    expect(() => run(["words", "data"], root)).toThrow("`--list` is needed once");
    expect(() => run(["words", "--list", "list.txt"], root)).toThrow("bad command line");
  });
});

describe("generic patterns", () => {
  const fake = (prefix: string, n: number) => prefix + "a1B2".repeat(n);

  test("hit personal paths, email addresses and token shapes", () => {
    const hits: [string, string][] = [
      ["home directory path", '"cwd":"/Users/jdoe/src"'],
      ["per-user temporary path", "/private/var/folders/zz/T/x"],
      ["email address", "mail someone@company.com now"],
      ["API key", fake("key sk-ant-", 6)],
      ["JWT", `${fake("eyJ", 3)}.${fake("", 3)}.${fake("", 3)}`],
      ["bearer token", "Authorization: Bearer abc.def-123456"],
      ["GitHub token", fake("ghp_", 9)],
      ["AWS access key", "AKIA" + "ABCDEFGHIJ234567"],
      ["Slack token", fake("xoxb-", 4)],
    ];
    for (const [rule, line] of hits) expect(patterns(line), line).toEqual([rule]);
  });

  test("pass placeholders", () => {
    for (const line of [
      '"cwd":"/Users/<user>/src"',
      "$HOME/.claude and /Users/Shared/x",
      "user@example.invalid, a@example.com, b@mail.example.org, c@host.test",
      "Authorization: Bearer <secret>",
      "sk-short and <tmpdir>/x",
    ]) {
      expect(patterns(line), line).toEqual([]);
    }
  });

  test("the scan command reports rules, not the text that matched", () => {
    const root = tree({
      "testdata/r.jsonl": '{"a":1}\n{"cwd":"/Users/jdoe/x","by":"jd@corp.net"}\n',
    });
    const { code, output } = run(["scan", "testdata"], root);
    expect(code).toBe(1);
    expect(output).toContain("testdata/r.jsonl:2: home directory path");
    expect(output).toContain("testdata/r.jsonl:2: email address");
    expect(output).not.toMatch(/jdoe|corp/);
  });
});
