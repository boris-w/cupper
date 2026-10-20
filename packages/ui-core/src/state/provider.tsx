// The state layer under the components (design 5.7.6): the UI store, the
// streams held by the subscription policy, and the query client.

import { QueryClientProvider } from "@tanstack/react-query";
import { createContext, use, useEffect, useMemo, useState, type ReactNode } from "react";
import { useStore } from "zustand";

import { required, useHost } from "../app/providers";
import { createQueryClient, invalidation } from "./query";
import { Frames } from "./stream";
import { Subscriptions } from "./subscriptions";
import type { UiStore } from "./ui";

const UiContext = createContext<UiStore | null>(null);
const StreamsContext = createContext<Subscriptions | null>(null);

export function UiProvider({ ui, children }: { ui: UiStore; children: ReactNode }) {
  return <UiContext value={ui}>{children}</UiContext>;
}

export const useUi = () => required(use(UiContext), "UiProvider");

/** Holds the streams of the session on screen and of the index; inside `HostProvider` and `UiProvider`. */
export function StateProvider({ children }: { children: ReactNode }) {
  const { client } = useHost();
  const ui = useUi();
  const [state] = useState(() => {
    const query = createQueryClient();
    const invalidate = invalidation(query, client);
    const streams = new Subscriptions(client, new Frames(), invalidate.onEvent);
    return { query, streams, off: invalidate.off };
  });
  const sessionId = useStore(ui, ({ route }) =>
    route.page === "session" ? route.sessionId : undefined,
  );
  useEffect(() => {
    state.streams.start();
    return () => state.streams.close();
  }, [state]);
  // While rendering, so that the session page below finds its store; `show`
  // is idempotent for the session on screen.
  useMemo(() => state.streams.show(sessionId), [state, sessionId]);
  useEffect(() => state.off, [state]);
  return (
    <StreamsContext value={state.streams}>
      <QueryClientProvider client={state.query}>{children}</QueryClientProvider>
    </StreamsContext>
  );
}

export const useStreams = () => required(use(StreamsContext), "StateProvider");
