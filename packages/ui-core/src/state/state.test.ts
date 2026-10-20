import { HostClient, type Validation } from "@cupper/host-client";
import { hostError, MockHost, type MockConnection } from "@cupper/host-client/mock";
import type { Handshake, ShellAPI } from "@cupper/shell-api";
import { describe, expect, test, vi } from "vitest";

import { LayoutStore, PREF_KEYS } from "../layout/store";
import { Frames } from "./stream";
import { Subscriptions } from "./subscriptions";

/** The mock host answers `initialize` only through `conn.initialized`; nothing reads the result. */
const host = () => new MockHost([{ method: "initialize", params: {}, result: {} }]);

function connect(mock: MockHost, client: HostClient): MockConnection {
  const conn = mock.connect();
  conn.initialized = true;
  client.attach(conn.port(), {} as Handshake);
  return conn;
}

const turn = {
  type: "turn.started",
  data: { turn_id: "t_1", turn_no: 1, origin: "user", started_at: 1 },
};
const item = {
  type: "item.started",
  data: { item_id: "i_1", turn_id: "t_1", started_at: 1, kind: "agent_message" },
};
const delta = (text: unknown) => ({
  type: "item.delta",
  data: { item_id: "i_1", field: "text", text },
});

/**
 * Subscriptions answered with a one-page snapshot at seq 1 of epoch 1, or a
 * replay when they come with a cursor; a frame is run by `frame()`.
 */
function setup(validation: Validation = "all") {
  const mock = host();
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream, after_seq } = params as { stream: string; after_seq?: number };
    const at = after_seq ?? 1;
    const mode = after_seq === undefined ? "snapshot" : "replay";
    conn.reply(id, { result: { stream, mode, at_seq: at, head_seq: at, epoch: 1 } });
    if (mode === "snapshot") {
      const ops = stream === "index" ? [] : [turn, item];
      const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops };
      conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    }
    return undefined;
  });
  const logs: string[] = [];
  const client = new HostClient({ validation, log: (message) => logs.push(message) });
  const frames: (() => void)[] = [];
  const streams = new Subscriptions(client, new Frames((cb) => frames.push(cb)));
  const conn = connect(mock, client);
  streams.start();
  const frame = () => frames.splice(0).forEach((cb) => cb());
  const events = (stream: string, seq: number, ...bodies: object[]) =>
    conn.send({
      jsonrpc: "2.0",
      method: "stream/events",
      params: { stream, events: bodies.map((body, i) => ({ seq: seq + i, v: 1, ts: 1, ...body })) },
    });
  return { mock, client, streams, frames, frame, events, logs, conn };
}

const subscribed = (mock: MockHost) =>
  mock.requests.filter((r) => r.method === "stream/subscribe").map((r) => r.params as object);

describe("UNIT-ui-store", () => {
  test("messages apply as they arrive; listeners hear once per frame, per entry", () => {
    const { streams, frames, frame, events } = setup();
    const store = streams.show("s_1");
    const view = store?.view;
    // Applied at once: complete before any frame ran, and one frame for all of it.
    expect([view?.complete, frames.length]).toEqual([true, 1]);
    frame();
    const heard: string[] = [];
    for (const key of ["item:i_1", "turn:t_1", "view"]) {
      view?.versions.subscribe(key, () => heard.push(key));
    }
    events("session/s_1", 2, delta("a"), delta("b"));
    events("session/s_1", 4, delta("c"));
    expect([JSON.parse(JSON.stringify(view)).items.i_1.text, heard]).toEqual(["abc", []]);
    expect(frames).toHaveLength(1);
    frame();
    expect(heard).toEqual(["item:i_1", "view"]);
  });

  test("the session on screen and 3 more stay subscribed; drafts and scroll outlive them", () => {
    const { mock, streams } = setup();
    const ui = new LayoutStore(noPrefs(), {}).ui;
    ui.getState().setDraft("s_1", "half a thought");
    ui.getState().setScroll("s_1", 480);
    const first = streams.show("s_1");
    for (const id of ["s_2", "s_3", "s_4", "s_5"]) {
      streams.show(id);
    }
    expect(streams.held()).toEqual(["s_2", "s_3", "s_4", "s_5"]);
    expect(mock.requests.at(-1)).toMatchObject({
      method: "stream/unsubscribe",
      params: { stream: "session/s_1" },
    });
    expect([ui.getState().drafts.s_1, ui.getState().scroll.s_1]).toEqual(["half a thought", 480]);
    // A hit moves the session up; the new session page keeps the 3 seen last.
    const second = streams.session("s_2");
    expect(streams.show("s_2")).toBe(second);
    streams.show(undefined);
    expect(streams.held()).toEqual(["s_4", "s_5", "s_2"]);
    // Back to a dropped session: a cache miss with a fresh view.
    expect(streams.show("s_1")).not.toBe(first);
    expect(
      subscribed(mock).filter((p) => "stream" in p && p.stream === "session/s_1"),
    ).toHaveLength(2);
  });

  test("a cache miss loads, and a failed subscription shows its error", () => {
    const { mock, streams } = setup();
    const waiting: (() => void)[] = [];
    mock.handle("stream/subscribe", ({ id, conn, params }) => {
      const { stream } = params as { stream: string };
      if (stream === "session/s_8") {
        return hostError("not_found");
      }
      const result = { stream, mode: "snapshot", at_seq: 1, head_seq: 1, epoch: 1 };
      const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops: [turn] };
      waiting.push(() => {
        conn.reply(id, { result });
        conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
      });
      return undefined;
    });
    const view = streams.show("s_9")?.view;
    expect(view?.complete).toBe(false);
    waiting.forEach((reply) => reply());
    expect(view?.complete).toBe(true);
    expect(streams.show("s_8")?.view.error?.data).toMatchObject({ code: "not_found" });
  });

  test("after a reconnect every held stream subscribes again from its cursor", () => {
    const { mock, client, streams, events } = setup();
    streams.show("s_1");
    events("session/s_1", 2, delta("a"));
    client.detach();
    connect(mock, client);
    expect(subscribed(mock).slice(2)).toEqual([
      { stream: "index", after_seq: 1, epoch: 1 },
      { stream: "session/s_1", after_seq: 2, epoch: 1 },
    ]);
    expect(streams.session("s_1")?.view.complete).toBe(true);
  });

  test("production builds check every event with zod but item.delta", () => {
    for (const validation of ["all", "non_delta"] as const) {
      const { streams, events, logs } = setup(validation);
      const view = streams.show("s_1")?.view;
      const broken = { ...turn, data: { ...turn.data, turn_no: "two" } };
      events("session/s_1", 2, delta(5), broken);
      const malformed = logs.filter((log) => log.includes("malformed"));
      const unknown = view?.unknown.map(({ body }) => body.type);
      if (validation === "all") {
        expect([malformed.length, unknown]).toEqual([2, ["item.delta", "turn.started"]]);
      } else {
        // Folded as it came: the text is what the host sent.
        expect([malformed.length, unknown]).toEqual([1, ["turn.started"]]);
        expect(JSON.parse(JSON.stringify(view)).items.i_1.text).toBe("5");
      }
    }
  });

  test("a client notification goes out while connected, and is dropped otherwise", () => {
    const { mock, client } = setup();
    client.notify("client/presence", { foreground: true });
    client.detach();
    client.notify("client/presence", { foreground: false });
    const sent = mock.requests.filter((r) => r.method === "client/presence");
    expect(sent.map((r) => [r.id, r.params])).toEqual([[undefined, { foreground: true }]]);
  });

  test("an echo of an earlier save is dropped while a later save waits for its own", async () => {
    let changed: (key: string, value: unknown) => void = () => undefined;
    let fail = false;
    const prefs: ShellAPI["prefs"] = {
      set: () => (fail ? Promise.reject(new Error("disk full")) : Promise.resolve()),
      onChange: (cb) => ((changed = cb), () => undefined),
    };
    const store = new LayoutStore(prefs, {});
    const key = PREF_KEYS.sidebarWidth;
    const width = () => store.getState().prefs.sidebarWidth;
    const seen: number[] = [];
    store.subscribe(() => seen.push(width()));
    for (const w of [304, 320, 336]) {
      store.set("sidebarWidth", w);
    }
    changed(key, 304);
    changed(key, 320);
    expect(width()).toBe(336);
    changed(key, 336);
    // Another window's change, with nothing of ours waiting, applies.
    changed(key, 400);
    expect(seen).toEqual([304, 320, 336, 336, 400]);
    // A failed save sends no echo: it does not hold back the next change.
    fail = true;
    store.set("sidebarWidth", 416);
    await Promise.resolve();
    await Promise.resolve();
    changed(key, 432);
    expect(width()).toBe(432);
  });
});

function noPrefs(): ShellAPI["prefs"] {
  return { set: () => Promise.resolve(), onChange: () => () => undefined };
}

test("Frames calls requestAnimationFrame as the browser requires, with the window as this", () => {
  const queued: (() => void)[] = [];
  vi.stubGlobal("requestAnimationFrame", function (this: unknown, cb: () => void) {
    if (this !== undefined && this !== globalThis) {
      throw new TypeError("Illegal invocation");
    }
    queued.push(cb);
  });
  try {
    const flush = vi.fn();
    new Frames().mark({ versions: { flush } } as unknown as Parameters<Frames["mark"]>[0]);
    queued.forEach((cb) => cb());
    expect(flush).toHaveBeenCalledOnce();
  } finally {
    vi.unstubAllGlobals();
  }
});
