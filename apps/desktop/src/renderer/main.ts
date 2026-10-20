// The main document: `@cupper/ui-core` on the Electron ShellAPI, which the
// preload puts on the main world's global object (design 5.1.2, 5.7.3). In
// e2e builds only, `window.__cupperTest` reads the host client's sync
// counters and state, and the views of the streams the window follows with
// the seq each reached (design 5.9.9, E2E-NFR1-1), and sends a command as the
// window (E2E-SEC); other builds do not contain it.

import "@cupper/ui-core/styles.css";

import type { HostClient, StreamSink } from "@cupper/host-client";
import { SHELL_GLOBAL, type ShellAPI } from "@cupper/shell-api";
import { mount } from "@cupper/ui-core";

const shell = (globalThis as unknown as Record<string, ShellAPI | undefined>)[SHELL_GLOBAL];
if (!shell) {
  throw new Error(`the preload did not expose window.${SHELL_GLOBAL}`);
}
const onClient =
  __CUPPER_FLAVOR__ === "e2e"
    ? (client: HostClient) => {
        const followed = new Map<string, { sink: StreamSink; seq?: number }>();
        // A snapshot's seq: its notification reaches this listener after the stream.
        const snapshots = new Map<string, number>();
        client.onNotification((method, params) => {
          const { stream, at_seq: at } = params as { stream: string; at_seq?: number };
          if (method === "stream/snapshot" && at !== undefined) snapshots.set(stream, at);
        });
        const subscribe = client.subscribe.bind(client);
        client.subscribe = (stream, sink) => {
          const entry: { sink: StreamSink; seq?: number } = { sink };
          followed.set(stream, entry);
          return subscribe(stream, {
            reset: () => {
              entry.seq = undefined;
              sink.reset();
            },
            apply: (body, event) => {
              entry.seq = event?.seq ?? entry.seq;
              sink.apply(body, event);
            },
            live: () => sink.live(),
            failed: (error) => sink.failed?.(error),
          });
        };
        const views = () =>
          Object.fromEntries(
            [...followed].map(([stream, { sink, seq }]) => {
              const view = (sink as { view?: { complete: boolean; toJSON(): unknown } }).view;
              const at = seq ?? snapshots.get(stream);
              return [stream, { seq: at, complete: view?.complete, view: view?.toJSON() }];
            }),
          );
        // A command as the window would send it, through the main process's
        // checks: its result, or the error the bridge or the host gave.
        const command = (method: string, params: Record<string, unknown>) =>
          client.command(method, params).then(
            (result) => ({ result }),
            (error: unknown) => ({ error: (error as { error?: unknown }).error ?? String(error) }),
          );
        const test = {
          counters: () => client.counters,
          state: () => client.state,
          views,
          command,
        };
        Object.assign(globalThis, { __cupperTest: test });
      }
    : undefined;
await mount(shell, undefined, onClient);
