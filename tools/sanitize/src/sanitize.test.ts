import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, test } from "vitest";

import { main } from "./main.ts";
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
const record = (frames: [string, object][]) =>
  [HEADER, ...frames.map(([dir, body], i) => ({ seq: i + 1, t: 0, instance: 0, dir, ...body }))]
    .map((line) => JSON.stringify(line) + "\n")
    .join("");
const RECORDING = record(FRAMES);

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

describe("UNIT-sanitize-codex-mcp", () => {
  const update = (u: object) => ({ msg: { method: "session/update", params: { update: u } } });
  // Made-up MCP servers, each named in one of the ways Codex names them, then in other spellings.
  const FRAMES: [string, object][] = [
    ["stderr", { data: "config: `mcp_servers.acme-tracker.type` is ignored" }],
    ["stderr", { data: "error=failed to refresh OAuth tokens for server zeta_docs: no grant" }],
    [
      "recv",
      update({
        sessionUpdate: "tool_call",
        title: "mcp.widget_hub.search",
        rawInput: { arguments: {} },
        _meta: { is_mcp_tool_call: true },
      }),
    ],
    ["recv", update({ sessionUpdate: "tool_call", rawInput: { server: "notesVault", tool: "f" } })],
    ["recv", update({ rawOutput: { _meta: { "codex/widgetHubExecutionDurationMs": 3 } } })],
    ["recv", update({ text: "ACME_TRACKER, Zeta Docs, notes-vault; a widget hubcap" })],
    [
      "recv",
      update({
        sessionUpdate: "available_commands_update",
        availableCommands: [
          { name: "$deploy", _meta: { jetbrains: { air: { skillPath: "/opt/acme/SKILL.md" } } } },
        ],
      }),
    ],
  ];

  test("redacts the server names in every spelling and the skill paths", () => {
    const { result, read } = sanitize({ "codex.jsonl": record(FRAMES) });
    expect(result.findings).toEqual([]);
    const text = read("codex.jsonl");
    expect(text).not.toMatch(/acme.?tracker|zeta.?docs|widget.?hub\b|notes.?vault|\/opt\//i);
    const [, config, oauth, title, raw, meta, prose, commands] = text
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l));
    expect(config.data).toBe("config: `mcp_servers.<redacted>.type` is ignored");
    expect(oauth.data).toBe("error=failed to refresh OAuth tokens for server <redacted>: no grant");
    expect(title.msg.params.update.title).toBe("mcp.<redacted>.search");
    expect(raw.msg.params.update.rawInput).toEqual({ server: "<redacted>", tool: "f" });
    expect(meta.msg.params.update.rawOutput._meta).toEqual({
      "codex/<redacted>ExecutionDurationMs": 3,
    });
    expect(prose.msg.params.update.text).toBe(
      "<redacted>, <redacted>, <redacted>; a widget hubcap",
    );
    expect(commands.msg.params.update.availableCommands[0]._meta.jetbrains.air).toEqual({
      skillPath: "<redacted skill path>",
    });
    expect(sanitize({ "codex.jsonl": text }).read("codex.jsonl")).toBe(text);
  });
});

describe("UNIT-sanitize-across-chunks", () => {
  const update = (u: object) => ["recv", { msg: { params: { sessionId: session, update: u } } }];
  const chunk = (kind: string, text: string, messageId?: string) =>
    update({ sessionUpdate: `agent_${kind}_chunk`, messageId, content: { type: "text", text } });
  const output = (data: string) =>
    update({
      sessionUpdate: "tool_call_update",
      toolCallId: "c1",
      _meta: { terminal_output: { data } },
    });
  // The temporary path over four message chunks, thoughts between them; a
  // local term over two thought chunks with no ID; a bearer token over three
  // deltas of a command's output. The last thought is another text.
  const FRAMES = [
    chunk("message", "Wrote [a.txt](/", "m1"),
    chunk("message", "var/fol", "m1"),
    chunk("thought", "Working for Ac"),
    chunk("thought", "me Corp"),
    chunk("thought", " today, Ac"),
    chunk("message", "ders/zz/qwerty", "m1"),
    chunk("message", "uiopasdfgh/T/run/a.txt)", "m1"),
    output("curl -H 'Authorization: Bear"),
    output("er a1B2c3D4"),
    output("e5F6g7H8' x\n"),
    chunk("thought", "me Corp"),
  ] as [string, object][];
  interface Update {
    content?: { text: string };
    _meta?: { terminal_output: { data: string } };
  }
  const text = ({ msg: { params } }: { msg: { params: { update: Update } } }) =>
    params.update.content?.text ?? params.update._meta?.terminal_output.data;

  test("redacts a match over several chunks into its first, keeping the frames and seq", () => {
    const { result, read } = sanitize({ "s.jsonl": record(FRAMES) });
    expect(result.findings).toEqual([]);
    const out = read("s.jsonl");
    const [, ...frames] = out
      .trim()
      .split("\n")
      .map((l) => JSON.parse(l));
    expect(frames.map((f) => f.seq)).toEqual(FRAMES.map((_, i) => i + 1));
    expect(frames.map(text)).toEqual([
      "Wrote [a.txt](<tmpdir>/",
      "",
      "Working for <redacted>",
      "",
      " today, Ac",
      "",
      "run/a.txt)",
      "curl -H 'Authorization: Bearer <secret>",
      "",
      "' x\n",
      "me Corp",
    ]);
    expect(sanitize({ "s.jsonl": out }).read("s.jsonl")).toBe(out);
  });

  test("the scans report the range of lines a match runs over, not the text", () => {
    const { root } = sanitize({ "s.jsonl": record(FRAMES), "list.txt": "acme corp\n" });
    const run = (...args: string[]) => {
      const lines: string[] = [];
      return [main(args, join(root, "in"), (l) => lines.push(l)), ...lines];
    };
    expect(run("scan", "s.jsonl")).toEqual([
      1,
      "s.jsonl:2-7: per-user temporary path",
      "s.jsonl:9-11: bearer token",
      "2 findings in 1 of 1 files",
    ]);
    expect(run("words", "--list", "list.txt", "s.jsonl")).toEqual([
      1,
      "s.jsonl:4-5: entry 1 of the sensitive word list",
      "1 findings in 1 of 1 files",
    ]);
  });
});
