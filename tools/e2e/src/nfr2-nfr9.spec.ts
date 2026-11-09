// E2E-NFR2-retry and E2E-NFR9 (design 5.9.4, NFR-2, NFR-9; M1b's exit ② ③):
// a command whose reply the dropped connection lost is sent again with its
// `command_id` and runs once; a session goes on, by a resume, after its
// adapter or its CLI died under a turn, the turn ended with what to do and
// the history whole. The fake agent records its calls (`--calls`).

import { randomUUID } from "node:crypto";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

import { expect, test, type Cupper } from "./fixture";
import { events, texts, untilEvent, type StoredEvent } from "./helpers";
import { id, onScreen } from "./nfr1";
import { asWindow } from "./security";

/** Has the fake agent record its calls in the test's root, before the app starts; the file. */
function recording(cupper: Cupper): string {
  const path = join(cupper.root, "test_config.json");
  const config = JSON.parse(readFileSync(path, "utf8")) as {
    agents: { claude: { args?: string[] } };
  };
  const calls = join(cupper.root, "calls.jsonl");
  config.agents.claude.args = ["--calls", calls];
  writeFileSync(path, JSON.stringify(config));
  return calls;
}

/** How often the agent got `method`, with prompt text `text` if given. */
function called(calls: string, method: string, text?: string): number {
  const lines = readFileSync(calls, "utf8").split("\n").filter(Boolean);
  const all = lines.map((line) => JSON.parse(line) as { method: string; text?: string });
  return all.filter((call) => call.method === method && (text ?? call.text) === call.text).length;
}

/** The users' messages of `stream`, in order, and whether an item started twice. */
function history(cupper: Cupper, stream: string) {
  const started = events(cupper, stream).filter((e) => e.type === "item.started");
  const said = started.filter((e) => e.data.kind === "user_message");
  const ids = started.map((e) => e.data.item_id);
  const text = (e: StoredEvent) => (e.data.content as { text: string }[])[0]?.text;
  return { messages: said.map(text), repeated: ids.length !== new Set(ids).size };
}

/**
 * The session on screen, its first turn over, when `script` breaks the next
 * one: the turn's end as stored, the error the window shows, and what came
 * of the message sent after it.
 */
async function broken(cupper: Cupper, script: string, code: string) {
  const calls = recording(cupper);
  const stream = await onScreen(cupper, "say first");
  const seq = events(cupper, stream).at(-1)?.seq ?? 0;
  await cupper.cli.ok(["send", id(stream), script]);
  const ended = await untilEvent(cupper, stream, (e) => e.type === "turn.completed" && e.seq > seq);
  const t = await texts(cupper);
  const alert = cupper.window.getByRole("alert").filter({ hasText: t(`stream:error_${code}`) });
  await expect(alert).toBeVisible();
  // What to do, while the failed turn is the last one.
  const buttons = await alert.getByRole("button").allTextContents();
  const again = await cupper.cli.run(["send", id(stream), "say again", "--wait"]);
  expect(again.code, again.stderr).toBe(0);
  await expect(cupper.window.locator("main").getByText("again", { exact: true })).toBeVisible();
  const statuses = events(cupper, stream)
    .filter((e) => e.type === "turn.completed" && e.seq > seq)
    .map((e) => e.data.status);
  return { calls, stream, ended: ended.data, buttons, statuses };
}

test("E2E-NFR2-retry: a command whose reply was lost is sent again and runs once", async ({
  cupper,
}) => {
  const calls = recording(cupper);
  const stream = await onScreen(cupper, "say first");
  const seq = events(cupper, stream).at(-1)?.seq ?? 0;
  const command = randomUUID();
  // The connection drops as the host's reply comes: the host has run it.
  const window = await cupper.app.browserWindow(cupper.window);
  const contents = await window.evaluate((w) => w.webContents.id);
  await cupper.app.evaluate(
    (_electron, [contents, command]) => {
      const hooks = (globalThis as { __cupperE2E?: { dropReplyTo(id: number, c: string): void } })
        .__cupperE2E;
      hooks?.dropReplyTo(contents, command);
    },
    [contents, command] as const,
  );
  const content = [{ type: "text", text: "say once" }];
  const params = { command_id: command, session_id: id(stream), mode: "queue", content };
  // The window's client sends it again on the next connection; the ledger answers.
  const sent = await asWindow(cupper, "session/send", params);
  expect(sent).toMatchObject({ result: { replayed: true } });
  await untilEvent(cupper, stream, (e) => e.type === "turn.completed" && e.seq > seq);
  expect(history(cupper, stream)).toEqual({ messages: ["say first", "say once"], repeated: false });
  expect(called(calls, "session/prompt", "say once")).toBe(1);
  const main = cupper.window.locator("main");
  await expect(main.getByText("once", { exact: true })).toHaveCount(1);
  await expect(main.getByText("say once", { exact: true })).toHaveCount(1);
});

test("E2E-NFR9-adapter-crash: the adapter dies under a turn; the session goes on in the next one", async ({
  cupper,
}) => {
  const script = "say working; sleep 100; exit 3";
  const { calls, stream, ended, buttons, statuses } = await broken(
    cupper,
    script,
    "adapter_exited",
  );
  expect(ended).toMatchObject({ status: "failed", error_code: "adapter_exited" });
  expect(buttons).toEqual([(await texts(cupper))("stream:retry")]);
  expect(statuses).toEqual(["failed", "completed"]);
  // Another adapter, which resumes the agent's session.
  expect(called(calls, "session/resume")).toBe(1);
  const messages = ["say first", script, "say again"];
  expect(history(cupper, stream)).toEqual({ messages, repeated: false });
});

test("E2E-NFR9-cli-crash: the CLI dies under a turn; the next message resumes the session", async ({
  cupper,
}) => {
  const { calls, stream, ended, buttons, statuses } = await broken(
    cupper,
    "crash_cli",
    "process_exited",
  );
  expect(ended).toMatchObject({ status: "failed", error_code: "process_exited" });
  expect(buttons).toEqual([(await texts(cupper))("stream:retry")]);
  expect(statuses).toEqual(["failed", "completed"]);
  // "Session not found" before any output: resumed, and sent once more (K6).
  expect(called(calls, "session/resume")).toBe(1);
  expect(called(calls, "session/prompt", "say again")).toBe(2);
  expect(called(calls, "session/prompt", "crash_cli")).toBe(1);
  const messages = ["say first", "crash_cli", "say again"];
  expect(history(cupper, stream)).toEqual({ messages, repeated: false });
});
