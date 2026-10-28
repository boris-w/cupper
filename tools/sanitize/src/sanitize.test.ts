import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, test } from "vitest";

import { sanitizeTree, type Machine } from "./sanitize.ts";

// A made-up machine; none of these names exist.
const machine: Machine = {
  home: "/Users/jdoe",
  user: "jdoe",
  tmp: "/var/folders/zz/qwertyuiopasdfgh/T/",
  hosts: ["jdoe-laptop"],
  bases: ["/tmp/cupper-run-x1"],
  terms: ["Acme Corp"],
};
const session = "0199e3a0-1234-4abc-8def-0123456789ab";

const HEADER = { format: "cupper-rec", v: 1, source: "host", scenario: "s01", usage: ["gold-a"] };
const FRAMES: [string, object][] = [
  ["note", { note: { event: "instance_started", host: "jdoe-laptop" } }],
  ["send", { msg: { id: 1, params: { cwd: "/tmp/cupper-run-x1/repos/a", sessionId: session } } }],
  [
    "recv",
    {
      msg: {
        method: "session/update",
        params: {
          sessionId: session,
          update: {
            sessionUpdate: "available_commands_update",
            availableCommands: [
              { name: "review", description: "Review a pull request" },
              { name: "deploy-acme", description: "Ship it (user)", input: { hint: "[acme env]" } },
              { name: "notes:sync", description: "Sync notes" },
            ],
          },
        },
      },
    },
  ],
  [
    "recv",
    {
      msg: {
        method: "_auth/status_update",
        params: {
          authStatus: {
            kind: "account",
            account: { plan: "max", email: "jdoe@acme.dev", org: "Acme Corp" },
          },
        },
      },
    },
  ],
  ["stderr", { data: "read /Users/jdoe/.claude; mcp__acme-tracker__search; JDOE-LAPTOP.local" }],
  [
    "note",
    {
      note: {
        event: "procs",
        procs: [
          { pid: 7, label: "node index.js" },
          { pid: 8, label: "/opt/x" },
        ],
      },
    },
  ],
  [
    "recv",
    {
      msg: {
        result: {
          content: [{ mimeType: "image/png", data: "iVBORw0K".repeat(10) }],
          signature: "c2ln",
        },
      },
    },
  ],
];
const RECORDING = [
  HEADER,
  ...FRAMES.map(([dir, body], i) => ({ seq: i + 1, t: 0, instance: 0, dir, ...body })),
]
  .map((line) => JSON.stringify(line) + "\n")
  .join("");

function sanitize(files: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), "sanitize-"));
  for (const [path, text] of Object.entries(files)) {
    mkdirSync(join(root, "in", path, ".."), { recursive: true });
    writeFileSync(join(root, "in", path), text);
  }
  const result = sanitizeTree(join(root, "in"), join(root, "out"), machine);
  const read = (path: string) => readFileSync(join(root, "out", path), "utf8");
  return { root, result, read };
}

describe("sanitize", () => {
  test("redacts the machine, accounts, user tools and session IDs, keeping the format", () => {
    const { result, read } = sanitize({ "claude-agent-acp/0.91.0/2.1.296/s01.jsonl": RECORDING });
    expect(result).toEqual({ files: ["claude-agent-acp/0.91.0/2.1.296/s01.jsonl"], findings: [] });
    const lines = read("claude-agent-acp/0.91.0/2.1.296/s01.jsonl")
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l));
    expect(lines[0]).toEqual(HEADER);
    expect(lines[1].note).toEqual({ event: "instance_started", host: "<host>" });
    expect(lines[2].msg.params).toEqual({ cwd: "<tmp>/repos/a", sessionId: "session-01" });
    expect(
      lines[3].msg.params.update.availableCommands.map((c: { name: string }) => c.name),
    ).toEqual(["review", "user-skill-1", "user-skill-2"]);
    expect(lines[3].msg.params.update.availableCommands[1].input).toEqual({ hint: "<redacted>" });
    expect(lines[4].msg.params.authStatus.account).toEqual({
      plan: "max",
      email: "<email>",
      org: "<org>",
    });
    expect(lines[5].data).toBe("read $HOME/.claude; mcp__<redacted>; <host>.local");
    expect(lines[6].note.procs.map((p: { label: string }) => p.label)).toEqual([
      "adapter (node)",
      "user-configured tool or MCP server",
    ]);
    expect(lines[7].msg.result.content[0].data).toBe("iVBORw0K".repeat(10));
    expect(lines[7].msg.result.signature).toBe("<signature>");
  });

  test("changes nothing on its own output", () => {
    const first = sanitize({ "a/s.jsonl": RECORDING });
    const output = first.read("a/s.jsonl");
    const second = sanitize({ "a/s.jsonl": output });
    expect(second.result.findings).toEqual([]);
    expect(second.read("a/s.jsonl")).toBe(output);
  });

  test("fails the scan of its output on what no rule redacts", () => {
    const line = { seq: 1, t: 0, instance: 0, dir: "stderr", data: "cd /Users/someone-else/work" };
    const { result } = sanitize({ "s.jsonl": JSON.stringify(line) + "\n" });
    expect(result.findings).toEqual([{ file: "s.jsonl", line: 1, rule: "home directory path" }]);
  });
});
