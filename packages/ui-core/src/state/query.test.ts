import { HostClient } from "@cupper/host-client";
import { MockHost } from "@cupper/host-client/mock";
import type { EventBody } from "@cupper/protocol";
import type { Handshake } from "@cupper/shell-api";
import { describe, expect, test } from "vitest";

import { ARCHIVED, createQueryClient, invalidation, staleKeys } from "./query";
import { Frames } from "./stream";
import { Subscriptions } from "./subscriptions";

const turn = { turn_id: "t_1", turn_no: 1, origin: "user", started_at: 1 };
const completed = {
  type: "turn.completed",
  data: { turn_id: "t_1", status: "completed", ended_at: 2, duration_ms: 1, final_item_ids: [] },
};
const agents = { type: "agents.updated", data: { agents: [], git: {} } };

/** Each stream comes as a one-page snapshot at seq 1; a session's holds turn `t_1`. */
function connected() {
  const mock = new MockHost([
    { method: "initialize", params: {}, result: {} },
    { method: "session/list_archived", params: {}, result: { sessions: [] } },
  ]);
  mock.handle("stream/subscribe", ({ id, conn, params }) => {
    const { stream, after_seq } = params as { stream: string; after_seq?: number };
    const mode = after_seq === undefined ? "snapshot" : "replay";
    conn.reply(id, { result: { stream, mode, at_seq: 1, head_seq: 1, epoch: 1 } });
    const ops = stream === "index" ? [] : [{ type: "turn.started", data: turn }];
    const page = { stream, at_seq: 1, epoch: 1, page: 0, last: true, ops };
    conn.send({ jsonrpc: "2.0", method: "stream/snapshot", params: page });
    return undefined;
  });
  const client = new HostClient({ log: () => undefined });
  const connect = () => {
    const conn = mock.connect();
    conn.initialized = true;
    client.attach(conn.port(), {} as Handshake);
    return conn;
  };
  const conn = connect();
  const send = (stream: string, body: object) =>
    conn.send({
      jsonrpc: "2.0",
      method: "stream/events",
      params: { stream, events: [{ seq: 2, v: 1, ts: 1, ...body }] },
    });
  return { client, connect, send };
}

describe("UNIT-ui-query", () => {
  test("which events make which queries stale", () => {
    const body = (type: string) => ({ type, data: {} }) as EventBody;
    expect(staleKeys("session/s_1", body("turn.completed"))).toEqual([["session", "s_1"]]);
    expect(staleKeys("index", body("turn.completed"))).toEqual([]);
    expect(staleKeys("index", body("session.removed"))).toEqual([ARCHIVED]);
    expect(staleKeys("index", body("session.upserted"))).toEqual([ARCHIVED]);
    expect(staleKeys("index", body("agents.updated"))).toEqual([["agents/capabilities"]]);
    expect(staleKeys("session/s_1", body("item.delta"))).toEqual([]);
  });

  test("live events invalidate their queries, a new connection all of them", async () => {
    const { client, connect, send } = connected();
    const query = createQueryClient();
    const { onEvent, off } = invalidation(query, client);
    const streams = new Subscriptions(client, new Frames(() => undefined), onEvent);
    streams.start();
    streams.show("s_1");
    const archived = query.fetchQuery({
      queryKey: ARCHIVED,
      queryFn: () => client.request("session/list_archived", {}),
    });
    expect(await archived).toEqual({ sessions: [] });
    const keys = [ARCHIVED, ["agents/capabilities"], ["session", "s_1", "git/status"]];
    const diffOfOther = ["session", "s_2", "git/status"];
    [...keys.slice(1), diffOfOther].forEach((key) => query.setQueryData(key, {}));
    const stale = () => keys.map((key) => query.getQueryState(key)?.isInvalidated);
    // The snapshots came before: only live events count.
    expect(stale()).toEqual([false, false, false]);

    send("session/s_1", completed);
    send("index", agents);
    expect(stale()).toEqual([false, true, true]);
    expect(query.getQueryState(diffOfOther)?.isInvalidated).toBe(false);

    keys.forEach((key) => query.setQueryData(key, {}));
    client.detach();
    connect();
    expect(stale()).toEqual([true, true, true]);
    off();
    streams.close();
  });
});
