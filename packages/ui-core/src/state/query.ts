// Request-response data under TanStack Query (design 5.7.6): `git/*`,
// `files/search`, `agents/capabilities`, `session/list_archived`. Nothing
// goes stale by time; the streams' events say what changed, and a new
// connection may have missed some, so it makes everything stale.
//
// Keys: `[method, ...]` for host-wide data, `["session", id, method, ...]`
// for a session's, so that one event invalidates all of the session's.

import type { ClientState, HostClient } from "@cupper/host-client";
import type { EventBody, SessionListArchivedResult } from "@cupper/protocol";
import { QueryClient, useQuery, type QueryKey } from "@tanstack/react-query";

import { useHost } from "../app/providers";

export const ARCHIVED: QueryKey = ["session/list_archived"];

export const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: { staleTime: Infinity, retry: false, refetchOnWindowFocus: false },
    },
  });

/** The queries an event on `stream` makes stale. */
export function staleKeys(stream: string, body: EventBody): QueryKey[] {
  const session = stream.startsWith("session/") ? stream.slice("session/".length) : undefined;
  switch (body.type) {
    // The turn changed files: the session's git status and diffs.
    case "turn.completed":
      return session === undefined ? [] : [["session", session]];
    case "session.upserted":
    case "session.removed":
      return [ARCHIVED];
    case "agents.updated":
      return [["agents/capabilities"]];
    default:
      return [];
  }
}

/** Invalidates by the streams' events, and everything on each new connection. */
export function invalidation(query: QueryClient, client: HostClient) {
  let ready = client.state.status === "ready";
  const offState = client.onState((state: ClientState) => {
    if (state.status === "ready" && !ready) {
      void query.invalidateQueries();
    }
    ready = state.status === "ready";
  });
  const onEvent = (stream: string, body: EventBody) =>
    staleKeys(stream, body).forEach((queryKey) => void query.invalidateQueries({ queryKey }));
  return { onEvent, off: offState };
}

/** The example query: the first page of archived sessions. */
export function useArchivedSessions() {
  const { client } = useHost();
  return useQuery({
    queryKey: ARCHIVED,
    queryFn: () => client.request<SessionListArchivedResult>("session/list_archived", {}),
  });
}
