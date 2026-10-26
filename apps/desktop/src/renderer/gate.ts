// The bridge gate's hooks in the renderer (T078, design 5.7.1), e2e builds
// only, as `window.__cupperGate`: what each stream brought once counting
// starts, more streams than the window holds, a blob through `getBlob`, and
// a resync of a whole session from its first event, which the T080 store
// applies as it arrives. The client
// acknowledges every chunk it took in, these included.

import { SessionView, type HostClient } from "@cupper/host-client";
import type { Event } from "@cupper/protocol";
import { Frames, StreamStore } from "@cupper/ui-core";

interface Tally {
  frames: number;
  events: number;
  /** One frame in 16 is measured, to keep the measuring light. */
  sampled: number;
  bytes: number;
}

interface Subscribed {
  epoch: number;
  head_seq: number;
}

export function gateHooks(client: HostClient) {
  let tallies: Map<string, Tally> | undefined;
  const listeners = new Set<(stream: string, events: Event[]) => void>();
  client.onNotification((method, params) => {
    if (method !== "stream/events") return;
    const { stream, events } = params as { stream: string; events: Event[] };
    listeners.forEach((listener) => listener(stream, events));
    if (!tallies) return;
    const tally = tallies.get(stream) ?? { frames: 0, events: 0, sampled: 0, bytes: 0 };
    tallies.set(stream, tally);
    tally.frames += 1;
    tally.events += events.length;
    if (tally.frames % 16 === 1) {
      tally.sampled += 1;
      tally.bytes += JSON.stringify(params).length;
    }
  });
  return {
    count: () => void (tallies = new Map()),
    /** Follows `stream` into a store of its own, beyond the sessions the window holds. */
    follow: (stream: string) =>
      void client.subscribe(stream, new StreamStore(stream, new SessionView(), new Frames())),
    tallies: () => Object.fromEntries(tallies ?? []),
    resets: () => client.counters.resets,
    async getBlob(hash: string) {
      const started = performance.now();
      const { length } = await client.getBlob({ hash });
      return { bytes: length, ms: performance.now() - started };
    },
    /** `stream` replayed from its first event (a cursor at 0), into a store of its own. */
    async resync(stream: string) {
      const { epoch, head_seq: head } = await client.request<Subscribed>("stream/subscribe", {
        stream,
      });
      await client.request("stream/unsubscribe", { stream });
      const store = new StreamStore(stream, new SessionView(), new Frames());
      let events = 0;
      const started = performance.now();
      const replayed = new Promise<void>((done) => {
        const apply = (at: string, batch: Event[]) => {
          if (at !== stream) return;
          batch.forEach((event) => store.apply(event, event));
          events += batch.length;
          if ((batch.at(-1)?.seq ?? 0) >= head) {
            listeners.delete(apply);
            done();
          }
        };
        listeners.add(apply);
      });
      await client.request("stream/subscribe", { stream, after_seq: 0, epoch });
      await replayed;
      const ms = performance.now() - started;
      await client.request("stream/unsubscribe", { stream });
      return { events, head, ms, items: Object.keys(store.view.toJSON().items).length };
    },
  };
}
