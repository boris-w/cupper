// NFR-1's acceptance (design 5.9.4): the desktop and the CLI are two clients
// of one host and stay in step. E2E-NFR1-5 is in `nfr1-restart.spec.ts`.

import type { Output } from "./cli";
import { expect, test, type Cupper } from "./fixture";
import { disconnectWindow, elevate, events, hostClient, texts, untilEvent } from "./helpers";
import { activatePanel, focusWindow, panelReady, pressDigit, requestPanel } from "./input";
import { asked, bothViews, follow, id, openSession, pattern, session } from "./nfr1";

/** A session on screen whose first turn is over; its stream. */
async function onScreen(cupper: Cupper, script: string): Promise<string> {
  await cupper.launch();
  await cupper.connected();
  const stream = await session(cupper, script);
  await untilEvent(cupper, stream, (event) => event.type === "turn.completed");
  await openSession(cupper, script);
  return stream;
}

/** The seq of the next `type` in `stream` after `seq`. */
const next = async (cupper: Cupper, stream: string, type: string, seq: number) =>
  (await untilEvent(cupper, stream, (event) => event.type === type && event.seq > seq)).seq;

test(
  "E2E-NFR1-1: the window and the CLI see the same session list and session",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say hello; think a thought; tool t1 read README.md");
    for (const followed of ["index", stream]) {
      const { window, cli } = await bothViews(cupper, followed, follow(cupper, followed));
      expect(window).toEqual(cli.view);
    }
  },
);

test(
  "E2E-NFR1-2: what the CLI sends, queues, interrupts, approves and answers shows in the window",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const [s, main, t] = [id(stream), cupper.window.locator("main"), await texts(cupper)];
    let seq = events(cupper, stream).at(-1)?.seq ?? 0;
    // Sent: a new turn.
    await cupper.cli.ok(["send", s, "say second", "--wait"]);
    await expect(main.getByText("say second", { exact: true })).toBeVisible();
    seq = events(cupper, stream).at(-1)?.seq ?? seq;
    // Queued behind a running turn; the running one interrupted says it was stopped.
    await cupper.cli.ok(["send", s, "wait_cancel"]);
    seq = await next(cupper, stream, "turn.started", seq);
    await cupper.cli.ok(["send", s, "say queued"]);
    await expect(main.locator("[data-queue-id]")).toHaveText("say queued");
    // Its reply may be lost to the turn's end (T059): the window tells. The
    // queue pauses until the next message (design 5.2.4).
    await cupper.cli.run(["interrupt", s]);
    const stopped = await pattern(cupper, "stream:turn_cancelled", "duration");
    await expect(main.getByText(stopped)).toBeVisible();
    await expect(main.locator("[data-queue-paused] [data-queue-id]")).toHaveText("say queued");
    // Approved and answered: the panel goes; the record line says how it ended.
    const approved = await pattern(cupper, "request:record_approved", "title");
    const answered = t("request:record_asked", { count: 1 });
    for (const [script, answer, record] of [
      ["permission p1", ["approve"], approved],
      ["question colour?", ["answer", "--field", "answer=blue"], answered],
    ] as const) {
      await cupper.cli.ok(["send", s, script]);
      const request = await untilEvent(
        cupper,
        stream,
        (e) => e.type === "interaction.requested" && e.seq > seq,
      );
      seq = request.seq;
      const requestId = String(request.data.request_id);
      await expect(requestPanel(cupper)).toBeVisible();
      await cupper.cli.ok([answer[0], s, requestId, ...answer.slice(1)]);
      await expect(main.locator(`[data-request-record="${requestId}"]`)).toHaveText(record);
    }
    await expect(requestPanel(cupper)).toHaveCount(0);
  },
);

test(
  "E2E-NFR1-2: as the user, the CLI elevates through the window's dialog, then sends",
  { tag: "@focus" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    const repl = await elevate(cupper, ["write"]);
    const sent = await repl.run(`send ${id(stream)} "say from the user" --wait`, 20_000);
    expect(sent.code, sent.stderr).toBe(0);
    await expect(
      cupper.window.locator("main").getByText("say from the user", { exact: true }),
    ).toBeVisible();
  },
);

test(
  "E2E-NFR1-3: approved in the window, the CLI sees it resolved",
  { tag: "@focus" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    await cupper.cli.ok(["send", id(stream), "permission p1"]);
    const request = await asked(cupper, stream);
    const cli = follow(cupper, stream);
    await panelReady(cupper);
    await focusWindow(cupper);
    await activatePanel(cupper);
    await pressDigit(cupper, 1);
    await cli.until((line) => {
      const view = line.view as { requests: Record<string, { resolved?: { status: string } }> };
      return view.requests[request]?.resolved?.status === "resolved";
    });
  },
);

test(
  "E2E-NFR1-3: approved by the CLI, the window's panel folds and says where",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    await cupper.cli.ok(["send", id(stream), "permission p1"]);
    const request = await asked(cupper, stream);
    await panelReady(cupper);
    await cupper.cli.ok(["approve", id(stream), request]);
    await expect(requestPanel(cupper)).toHaveCount(0);
    const t = await texts(cupper);
    await expect(
      cupper.window.getByText(t("request:handled_elsewhere", { client: "cupper" })),
    ).toBeVisible();
  },
);

test(
  "E2E-NFR1-3: answered by both at once, one answer counts",
  { tag: "@focus" },
  async ({ cupper }) => {
    const stream = await onScreen(cupper, "say first");
    await cupper.cli.ok(["send", id(stream), "permission p1"]);
    const request = await asked(cupper, stream);
    await panelReady(cupper);
    await focusWindow(cupper);
    await activatePanel(cupper);
    const [, cli] = await Promise.all([
      pressDigit(cupper, 1),
      cupper.cli.run(["approve", id(stream), request]),
    ]);
    const resolved = (await untilEvent(cupper, stream, (e) => e.type === "interaction.resolved"))
      .data;
    await expect(requestPanel(cupper)).toHaveCount(0);
    if (cli.code === 0) {
      // The CLI's counted; the window's, if it was sent, was told it came second.
      expect(resolved).toMatchObject({
        request_id: request,
        resolved_by: { client_name: "cupper" },
      });
    } else {
      expect(cli.lines.at(-1)).toMatchObject({ error: { reason: "already_resolved" } });
      expect(resolved).toMatchObject({
        request_id: request,
        resolved_by: { client_name: "Cupper" },
      });
    }
    const twice = events(cupper, stream).filter((e) => e.type === "interaction.resolved");
    expect(twice).toHaveLength(1);
  },
);

/**
 * A session whose turn writes text for a while; ranges of it come as
 * coalesced `item.delta` events (`from`). Returns once the text flows.
 */
async function flooding(cupper: Cupper): Promise<[string, number]> {
  const stream = await onScreen(cupper, "say first");
  const seq = events(cupper, stream).at(-1)?.seq ?? 0;
  await cupper.cli.ok(["send", id(stream), "flood_rate 40 5 30"]);
  await untilEvent(cupper, stream, (e) => e.type === "item.delta" && e.seq > seq);
  return [stream, seq];
}

/** At the end of the flood both views are the same, and neither client lost or redid a step. */
async function caughtUp(cupper: Cupper, [stream, seq]: [string, number], cli: Output) {
  await untilEvent(cupper, stream, (e) => e.type === "turn.completed" && e.seq > seq);
  const { window, cli: line } = await bothViews(cupper, stream, cli);
  expect(window).toEqual(line.view);
  expect(cli.lines.every((l) => l.resubscribed === 0)).toBe(true);
  expect(await hostClient(cupper).counters()).toEqual({ gaps: 0, duplicates: 0, resets: 0 });
}

test(
  "E2E-NFR1-4: a CLI that drops out mid-stream catches up from a snapshot",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const flood = await flooding(cupper);
    const first = follow(cupper, flood[0]);
    await first.until((line) => (line.seq as number) > 0);
    first.stop();
    expect(await first.closed).not.toBe(0);
    expect(first.lines.every((line) => line.resubscribed === 0)).toBe(true);
    await cupper.window.waitForTimeout(1000);
    await caughtUp(cupper, flood, follow(cupper, flood[0]));
  },
);

test(
  "E2E-NFR1-4: a reloaded window catches up from a snapshot",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const flood = await flooding(cupper);
    const cli = follow(cupper, flood[0]);
    await cupper.window.reload();
    await openSession(cupper, "say first");
    await caughtUp(cupper, flood, cli);
  },
);

test(
  "E2E-NFR1-4: a window whose connection dropped resumes from its cursor",
  { tag: "@smoke" },
  async ({ cupper }) => {
    const flood = await flooding(cupper);
    const cli = follow(cupper, flood[0]);
    await disconnectWindow(cupper);
    await caughtUp(cupper, flood, cli);
  },
);

test(
  "E2E-NFR1-4: after a reload, earlier history comes in front without moving the view",
  { tag: "@smoke" },
  async ({ cupper }) => {
    // More turns than a snapshot holds (`snapshot_turns`, 20).
    const stream = await onScreen(cupper, "say t1");
    for (let n = 2; n <= 23; n++) {
      await cupper.cli.ok(["send", id(stream), `say t${n}`, "--wait"]);
    }
    await cupper.window.reload();
    await openSession(cupper, "say t1");
    const main = cupper.window.locator("main");
    // The window's first turn, at the top of the list.
    await expect(main.getByText("say t23", { exact: true })).toBeVisible();
    await main.locator("[data-stream-list]").evaluate((list) => (list.scrollTop = 0));
    const anchor = main.getByText("say t4", { exact: true });
    await expect(anchor).toBeVisible();
    await expect(main.getByText("say t3", { exact: true })).toHaveCount(0);
    const before = await anchor.boundingBox();
    const t = await texts(cupper);
    await main.getByRole("button", { name: t("stream:history_more") }).click();
    await expect(main.getByText("say t3", { exact: true })).toHaveCount(1);
    // Not moved by what came in front (the history control used to be held, and
    // the view moved by the whole page). The rows in front can still be off by
    // their estimated height at the moment the view is put back: reported to
    // T082's follow-up, not asserted here.
    const moved = Math.abs(((await anchor.boundingBox())?.y ?? 0) - (before?.y ?? 0));
    expect(moved).toBeLessThan(40);
  },
);
