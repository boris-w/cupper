// Request-response data under TanStack Query (design 5.7.6): `git/*`,
// `files/search`, `agents/capabilities`, `session/list_archived`. Nothing
// goes stale by time; the streams' events say what changed, and a new
// connection may have missed some, so it makes everything stale.
//
// Keys: `[method, ...]` for host-wide data, `["session", id, method, ...]`
// for a session's, so that one event invalidates all of the session's.

import type { ClientState, HostClient } from "@cupper/host-client";
import {
  isKnownEvent,
  type AgentStatus,
  type ArchivedCursor,
  type EventBody,
  type SessionListArchivedResult,
} from "@cupper/protocol";
import { QueryClient, useInfiniteQuery, type QueryKey } from "@tanstack/react-query";

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
    default:
      return [];
  }
}

/** What an agent's cached capabilities follow: its versions and the host's count of changes (T268). */
const cacheOf = ({ adapter, cli, capabilities_rev }: AgentStatus) =>
  JSON.stringify([adapter.version, cli?.version, capabilities_rev]);

/**
 * Invalidates by the streams' events, and everything on each new connection.
 * `agents.updated` makes stale only the capabilities of an agent whose cache
 * may have changed, not at each resource sample.
 */
export function invalidation(query: QueryClient, client: HostClient) {
  let ready = client.state.status === "ready";
  /** Each agent's `cacheOf` as the live events last said; unknown on a new connection. */
  let caches = new Map<string, string>();
  const offState = client.onState((state: ClientState) => {
    if (state.status === "ready" && !ready) {
      caches = new Map();
      void query.invalidateQueries();
    }
    ready = state.status === "ready";
  });
  const invalidate = (queryKey: QueryKey) => void query.invalidateQueries({ queryKey });
  const onEvent = (stream: string, body: EventBody) => {
    staleKeys(stream, body).forEach(invalidate);
    if (body.type === "agents.updated" && isKnownEvent(body)) {
      for (const agent of body.data.agents) {
        const cache = cacheOf(agent);
        if (caches.get(agent.agent) !== cache) invalidate(["agents/capabilities", agent.agent]);
        caches.set(agent.agent, cache);
      }
    }
  };
  return { onEvent, off: offState };
}

/** The archived sessions, the latest first, a page at a time after the last one's cursor. */
export function useArchivedSessions() {
  const { client } = useHost();
  return useInfiniteQuery({
    queryKey: ARCHIVED,
    queryFn: ({ pageParam: cursor }) =>
      client.request<SessionListArchivedResult>("session/list_archived", cursor && { cursor }),
    initialPageParam: undefined as ArchivedCursor | undefined,
    getNextPageParam: (page) => page.next,
  });
}
