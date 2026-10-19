import type { EventBody } from "@cupper/protocol";
import { describe, expect, test } from "vitest";

import { HostClient } from "./client";
import { MockHost } from "./mock/host";
import { createMockShellHost } from "./mock/port";
import { connectShell } from "./shell";
import { StreamSync, type StreamSink, type SyncCounters } from "./sync";
import { fixtures } from "./testing";

const STREAM = "session/s_1";

const turn = (n: number) => ({
  type: "turn.started",
  data: { turn_id: `t_${n}`, turn_no: n, origin: "user", started_at: n },
});
const item = (id: string) => ({
  type: "item.started",
  data: { item_id: id, turn_id: "t_1", started_at: 1, kind: "agent_message" },
});
const delta = (id: string, text: string) => ({
  type: "item.delta",
  data: { item_id: id, field: "text", text },
});
/** An event at `seq`; `from` makes it a range event. */
const event = (seq: number, body: object, from?: number) => ({ seq, from, v: 1, ts: 1, ...body });
const events = (...list: object[]) => ({ stream: STREAM, events: list });
const page = (at_seq: number, epoch: number, n: number, last: boolean, ops: object[]) => ({
  stream: STREAM,
  ...{ at_seq, epoch, page: n, last, ops },
});
const result = (mode: string, at_seq: number, epoch: number, reason?: string) => ({
  stream: STREAM,
  ...{ mode, at_seq, head_seq: at_seq, epoch, reason },
});

/** A sink that writes down what it is told; it cannot apply deltas of `i_9` (a ViewError). */
function recorder() {
  const seen: string[] = [];
  const name = (body: EventBody) => {
    const data = body.data as { item_id?: string; text?: string } | null;
    return `${body.type}${data?.text === undefined ? "" : ` ${data.text}`}`;
  };
  const sink: StreamSink = {
    reset: () => seen.push("reset"),
    apply: (body, ev) => {
      if ((body.data as { item_id?: string } | null)?.item_id === "i_9") {
        throw new Error("no item i_9");
      }
      seen.push(ev ? `${ev.seq}: ${name(body)}` : name(body));
    },
    live: () => seen.push("live"),
    failed: (error) => seen.push(`failed: ${error.message}`),
  };
  return { seen, sink };
}

function setup() {
  const counters: SyncCounters = { gaps: 0, duplicates: 0, resets: 0 };
  const logs: string[] = [];
  const { seen, sink } = recorder();
  const sync = new StreamSync(STREAM, sink, counters, (message) => logs.push(message));
  return { sync, seen, counters, logs };
}

/** Live at seq 3 of epoch 1, the log cleared. */
function live() {
  const it = setup();
  expect(it.sync.subscribe()).toEqual({ stream: STREAM });
  expect(it.sync.subscribed(result("snapshot", 2, 1))).toBeUndefined();
  expect(it.sync.snapshot(page(2, 1, 0, true, [turn(1), item("i_1")]))).toBeUndefined();
  expect(it.sync.events(events(event(3, delta("i_1", "a"))))).toBeUndefined();
  expect([it.sync.live, it.sync.cursor]).toEqual([true, { epoch: 1, seq: 3 }]);
  it.seen.length = 0;
  return it;
}

const WITH_CURSOR = (seq: number, epoch = 1) => ({ stream: STREAM, after_seq: seq, epoch });

describe("UNIT-hc-sync", () => {
  test("snapshot pages build the view; the cursor comes with the last one", () => {
    const { sync, seen } = setup();
    sync.subscribe();
    sync.subscribed(result("snapshot", 5, 1));
    expect(sync.snapshot(page(5, 1, 0, false, [turn(1), item("i_1")]))).toBeUndefined();
    expect([sync.live, sync.cursor]).toEqual([false, undefined]);
    expect(sync.snapshot(page(5, 1, 1, true, [delta("i_1", "a")]))).toBeUndefined();
    expect([sync.live, sync.cursor]).toEqual([true, { epoch: 1, seq: 5 }]);
    expect(seen).toEqual(["reset", "turn.started", "item.started", "item.delta a", "live"]);

    // A page of another `at_seq`, another epoch, or out of order starts over.
    for (const wrong of [
      page(6, 1, 0, true, []),
      page(5, 2, 0, true, []),
      page(5, 1, 1, true, []),
    ]) {
      const { sync } = setup();
      sync.subscribe();
      sync.subscribed(result("snapshot", 5, 1));
      expect(sync.snapshot(wrong)).toEqual({ stream: STREAM });
      expect(sync.live).toBe(false);
    }
  });

  test("any snapshot drops the view and cursor; only a replay that fits keeps them", () => {
    for (const [mode, reason] of [
      ["snapshot", undefined],
      ["snapshot", "cursor_expired"],
      ["snapshot", "cursor_ahead"],
      ["rebuild", "whatever"],
    ] as const) {
      const { sync, seen, counters } = live();
      expect(sync.subscribe()).toEqual(WITH_CURSOR(3));
      expect(sync.subscribed(result(mode, 9, 1, reason))).toBeUndefined();
      expect([sync.cursor, seen, counters.resets]).toEqual([undefined, ["reset"], 1]);
      sync.snapshot(page(9, 1, 0, true, []));
      expect(sync.cursor).toEqual({ epoch: 1, seq: 9 });
    }
    for (const [at, epoch] of [
      [2, 1],
      [3, 2],
    ]) {
      const { sync, seen } = live();
      sync.subscribe();
      expect(sync.subscribed(result("replay", at ?? 0, epoch ?? 0))).toEqual({ stream: STREAM });
      expect(seen).toEqual(["reset"]);
    }
    const { sync, seen } = live();
    sync.subscribe();
    expect(sync.subscribed(result("replay", 3, 1))).toBeUndefined();
    expect([sync.live, seen]).toEqual([true, ["live"]]);
  });

  test("a gap or a duplicate subscribes again with the cursor, and is counted", () => {
    const { sync, seen, counters } = live();
    const contiguous = events(event(4, delta("i_1", "b")), event(6, delta("i_1", "cd"), 5));
    expect(sync.events(contiguous)).toBeUndefined();
    expect([sync.cursor, seen]).toEqual([
      { epoch: 1, seq: 6 },
      ["4: item.delta b", "6: item.delta cd"],
    ]);
    const cases = [
      [event(8, delta("i_1", "x")), "gaps"],
      [event(6, delta("i_1", "x")), "duplicates"],
      [event(7, delta("i_1", "x"), 5), "duplicates"],
    ] as const;
    for (const [ev, counter] of cases) {
      const before = counters[counter];
      expect(sync.events(events(ev))).toEqual(WITH_CURSOR(6));
      expect(counters[counter]).toBe(before + 1);
      expect(sync.subscribed(result("replay", 6, 1))).toBeUndefined();
    }
    expect(sync.events(events(event(7, delta("i_1", "e"))))).toBeUndefined();
    expect(seen.at(-1)).toBe("7: item.delta e");
    expect(counters).toEqual({ gaps: 1, duplicates: 2, resets: 0 });
  });

  test("a ViewError or an unreadable envelope starts over without the cursor", () => {
    for (const bad of [
      event(4, delta("i_9", "b")),
      { ...event(4, delta("i_1", "b")), seq: undefined },
    ]) {
      const { sync, seen, counters } = live();
      expect(sync.events(events(bad))).toEqual({ stream: STREAM });
      expect([sync.live, sync.cursor, seen, counters.resets]).toEqual([
        false,
        undefined,
        ["reset"],
        1,
      ]);
    }
    // Inside a snapshot: a page the view cannot apply, and events before the last page.
    for (const bad of [page(5, 1, 0, true, [delta("i_9", "a")]), events(event(6, turn(2)))]) {
      const { sync } = live();
      sync.subscribe();
      sync.subscribed(result("snapshot", 5, 1));
      expect("ops" in bad ? sync.snapshot(bad) : sync.events(bad)).toEqual({ stream: STREAM });
    }
  });

  test("stream/reset keeps the cursor and is counted; while waiting it is ignored", () => {
    const { sync, seen, counters } = live();
    expect(sync.reset()).toEqual(WITH_CURSOR(3));
    expect(sync.reset()).toBeUndefined();
    expect(sync.subscribed(result("replay", 3, 1))).toBeUndefined();
    expect([sync.live, seen, counters]).toEqual([
      true,
      ["live"],
      { gaps: 0, duplicates: 0, resets: 1 },
    ]);
  });

  test("a new data_epoch resets the stream, and its reused seqs are no duplicates", () => {
    const { sync, seen, counters } = live();
    sync.subscribe();
    expect(sync.subscribed(result("snapshot", 1, 2, "epoch_changed"))).toBeUndefined();
    sync.snapshot(page(1, 2, 0, true, [turn(1)]));
    // Seqs 2 and 3 come again, now in epoch 2.
    expect(sync.events(events(event(2, item("i_1")), event(3, delta("i_1", "z"))))).toBeUndefined();
    expect(sync.cursor).toEqual({ epoch: 2, seq: 3 });
    expect(seen).toEqual(["reset", "turn.started", "live", "2: item.started", "3: item.delta z"]);
    expect(counters).toEqual({ gaps: 0, duplicates: 0, resets: 1 });
  });

  test("while waiting, the older subscription's data is ignored; a failure goes to the sink", () => {
    const { sync, seen } = live();
    expect(sync.subscribe()).toEqual(WITH_CURSOR(3));
    expect(sync.events(events(event(9, delta("i_9", "b"))))).toBeUndefined();
    expect(sync.snapshot(page(3, 1, 0, true, []))).toBeUndefined();
    sync.failed({ code: -32000, message: "not_found" });
    expect(sync.subscribed({ mode: "replay" })).toBeUndefined();
    expect([sync.cursor, seen]).toEqual([
      { epoch: 1, seq: 3 },
      ["failed: not_found", "failed: malformed stream/subscribe result"],
    ]);
  });

  test("a malformed event or op affects only itself", () => {
    const { sync, seen, logs } = live();
    const broken = { type: "item.delta", data: { item_id: "i_1" } };
    expect(sync.events(events(event(4, broken), event(5, delta("i_1", "b"))))).toBeUndefined();
    expect(seen).toEqual(["4: item.delta", "5: item.delta b"]);
    expect(logs).toEqual([`${STREAM}: malformed data in item.delta, shown as unknown`]);
    // An op without a type is skipped; an event without a seq ends the message after those before it.
    sync.subscribe();
    sync.subscribed(result("snapshot", 7, 1));
    sync.snapshot(page(7, 1, 0, true, [turn(1), { data: {} }, item("i_1")]));
    expect(seen.slice(2)).toEqual(["reset", "turn.started", "item.started", "live"]);
    const noSeq = { ...event(9, turn(2)), seq: undefined };
    expect(sync.events(events(event(8, turn(2)), noSeq))).toEqual({ stream: STREAM });
    expect(seen.slice(6)).toEqual(["8: turn.started", "reset"]);
  });

  test("HostClient.subscribe follows the host, and resumes from the cursor after a reconnect", async () => {
    const host = new MockHost(fixtures);
    host.handle("stream/subscribe", ({ id, conn, params }) => {
      const resume = (params as { after_seq?: number }).after_seq;
      conn.reply(id, {
        result: result(resume === undefined ? "snapshot" : "replay", resume ?? 2, 3),
      });
      if (resume === undefined) {
        conn.send({
          jsonrpc: "2.0",
          method: "stream/snapshot",
          params: page(2, 3, 0, true, [turn(1)]),
        });
      }
      const seq = (resume ?? 2) + 1;
      conn.send({
        jsonrpc: "2.0",
        method: "stream/events",
        params: events(event(seq, item(`i_${seq}`))),
      });
      return undefined;
    });
    const client = new HostClient({ log: () => undefined });
    const { seen, sink } = recorder();
    const unsubscribe = client.subscribe(STREAM, sink);
    await connectShell(createMockShellHost(host), client);
    const settle = async (n: number) => {
      for (let i = 0; i < 50 && seen.length < n; i++) {
        await new Promise((resolve) => setTimeout(resolve, 0));
      }
    };
    await settle(4);
    host.connections[0]?.close();
    await settle(6);
    expect(seen).toEqual([
      "reset",
      "turn.started",
      "live",
      "3: item.started",
      "live",
      "4: item.started",
    ]);
    const subscribes = host.requests.filter((r) => r.method === "stream/subscribe");
    expect(subscribes.map((r) => r.params)).toEqual([{ stream: STREAM }, WITH_CURSOR(3, 3)]);
    expect(client.counters).toEqual({ gaps: 0, duplicates: 0, resets: 0 });
    unsubscribe();
    expect(host.requests.at(-1)).toMatchObject({
      method: "stream/unsubscribe",
      params: { stream: STREAM },
    });
    expect(() => client.subscribe("index", sink)).not.toThrow();
    expect(() => client.subscribe("index", sink)).toThrow("subscribed already");
  });

  test("HostClient routes only its streams' data, and a stream unsubscribed meanwhile ignores its result", async () => {
    const host = new MockHost(fixtures);
    const waiting: (() => void)[] = [];
    host.handle("stream/subscribe", ({ id, conn }) => {
      waiting.push(() => conn.reply(id, { result: result("snapshot", 1, 1) }));
      return undefined;
    });
    const client = new HostClient({ log: () => undefined });
    await connectShell(createMockShellHost(host), client);
    const index = recorder();
    const session = recorder();
    client.subscribe("index", index.sink);
    client.subscribe(STREAM, session.sink)();
    waiting.forEach((reply) => reply());
    host.notify("stream/snapshot", { ...page(1, 1, 0, true, [turn(1)]), stream: "index" });
    host.notify("stream/snapshot", page(1, 1, 0, true, [turn(1)]));
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect([index.seen, session.seen]).toEqual([["reset", "turn.started", "live"], []]);
  });
});
