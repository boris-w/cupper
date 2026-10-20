// The message stream (design 5.7.6): the session view flattened into rows,
// virtualized row by row. Earlier history comes in front on request, without
// moving what is on screen; while at the bottom the list follows the stream,
// and once scrolled up it stops and offers a way back down.

import type { SessionView } from "@cupper/host-client";
import type { HistoryCursor } from "@cupper/protocol";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useLayoutEffect, useRef, useState } from "react";

import { useHost, useT } from "../app/providers";
import { ArrowUpIcon } from "../components/icons";
import { flatten, type History } from "../rows/flatten";
import { RowView } from "../rows/registry";
import { streamRows } from "../rows/stream";
import "../rows";
import { useStreams } from "../state/provider";
import { useEntry, type StreamStore } from "../state/stream";
import { StreamActions } from "./actions";
import { ScrollKeeper } from "./scroll";

export function StreamList({ sessionId }: { sessionId: string }) {
  const store = useStreams().session(sessionId);
  return store ? <Rows sessionId={sessionId} store={store} /> : <div className="min-h-0 flex-1" />;
}

/**
 * The history before the window, by the view's first turn: there is more
 * until turn 1 is in. A page that splits a turn goes on from its `next`.
 */
function useHistory(sessionId: string, store: StreamStore<SessionView>, first?: number) {
  const { client } = useHost();
  const [paging, setPaging] = useState<{
    next?: HistoryCursor;
    loading?: boolean;
    failed?: boolean;
  }>({});
  const state: History =
    first === undefined || first <= 1
      ? "none"
      : paging.loading
        ? "loading"
        : paging.failed
          ? "failed"
          : "more";
  const load = useCallback(() => {
    if (first === undefined || paging.loading) {
      return;
    }
    const { next } = paging;
    setPaging({ next, loading: true });
    const cursor = next?.turn_no === first ? next : { turn_no: first };
    client.historyPage(sessionId, store, cursor).then(
      (after) => setPaging(after ? { next: after } : {}),
      () => setPaging({ next, failed: true }),
    );
  }, [client, first, paging, sessionId, store]);
  return { state, load };
}

function Rows({ sessionId, store }: { sessionId: string; store: StreamStore<SessionView> }) {
  const t = useT();
  const view = store.view;
  useEntry(view, "view");
  const json = view.toJSON();
  const history = useHistory(sessionId, store, json.turns[0]?.started.turn_no);
  const rows = flatten(json, view.unknown, history.state);
  const scroller = useRef<HTMLDivElement>(null);
  const keeper = useRef(new ScrollKeeper()).current;
  const [stuck, setStuck] = useState(true);
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scroller.current,
    estimateSize: () => 44,
    getItemKey: (index) => rows[index]?.key ?? index,
    overscan: 8,
  });
  const total = virtualizer.getTotalSize();
  const locate = (key: string) => {
    const index = rows.findIndex((row) => row.key === key);
    const start = virtualizer.measurementsCache[index]?.start;
    return start === undefined ? undefined : { index, start };
  };

  useLayoutEffect(() => {
    const element = scroller.current;
    const restored = keeper.restore(locate);
    if (element && restored !== undefined) {
      element.scrollTop = restored;
    } else if (element && keeper.stuck) {
      element.scrollTop = element.scrollHeight;
    }
  });

  const loadEarlier = () => {
    const element = scroller.current;
    const top = element?.scrollTop ?? 0;
    const first = virtualizer.getVirtualItems().find((item) => item.end > top);
    if (first) {
      keeper.hold(String(first.key), first.index, first.start, top);
    }
    history.load();
  };
  const onScroll = () => {
    const element = scroller.current;
    if (element) {
      setStuck(keeper.scrolled(element.scrollTop, element.scrollHeight, element.clientHeight));
    }
  };
  const toBottom = () => {
    keeper.stuck = true;
    setStuck(true);
    virtualizer.scrollToOffset(total);
  };

  return (
    <StreamActions value={{ loadEarlier }}>
      <div className="relative min-h-0 flex-1">
        <div ref={scroller} onScroll={onScroll} className="h-full overflow-y-auto" data-stream-list>
          <div className="relative w-full" style={{ height: total }}>
            {virtualizer.getVirtualItems().map((item) => {
              const row = rows[item.index];
              return (
                row && (
                  <div
                    key={item.key}
                    data-index={item.index}
                    ref={virtualizer.measureElement}
                    className="absolute top-0 left-0 w-full pt-(--layout-item-gap)"
                    style={{ transform: `translateY(${item.start}px)` }}
                  >
                    <RowView registry={streamRows} row={row} />
                  </div>
                )
              );
            })}
          </div>
        </div>
        {!stuck && (
          <button
            type="button"
            aria-label={t("stream:to_bottom")}
            onClick={toBottom}
            className="absolute bottom-2 left-1/2 flex size-8 -translate-x-1/2 rotate-180 items-center justify-center rounded-full border border-border bg-bg shadow-sm hover:bg-hover"
          >
            <ArrowUpIcon />
          </button>
        )}
      </div>
    </StreamActions>
  );
}
