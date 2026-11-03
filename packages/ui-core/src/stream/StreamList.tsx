// The message stream (design 5.7.6): the session view flattened into rows,
// virtualized row by row. Earlier history comes in front on request, without
// moving what is on screen; while at the bottom the list follows the stream,
// and once scrolled up it stops and offers a way back down. A running turn's
// plan has a pill there that scrolls to it.

import type { SessionView } from "@cupper/host-client";
import type { HistoryCursor } from "@cupper/protocol";
import { elementScroll, useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useLayoutEffect, useRef, useState } from "react";

import { useHost } from "../app/providers";
import { flatten, type History } from "../rows/flatten";
import { RowView } from "../rows/registry";
import { streamRows } from "../rows/stream";
import "../rows";
import { useStreams } from "../state/provider";
import { useEntry, type StreamStore } from "../state/stream";
import { StreamActions } from "./actions";
import { ComposerOverlays, runningPlan } from "./ComposerOverlays";
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
    // Its own scrolls, as it keeps rows in place while they are measured.
    scrollToFn: (offset, options, instance) => {
      elementScroll(offset, options, instance);
      keeper.put(instance.scrollElement?.scrollTop ?? offset);
    },
  });
  const total = virtualizer.getTotalSize();
  const locate = (key: string) => {
    const index = rows.findIndex((row) => row.key === key);
    const start = virtualizer.measurementsCache[index]?.start;
    return start === undefined ? undefined : { index, start };
  };

  useLayoutEffect(() => {
    const element = scroller.current;
    if (!element) {
      return;
    }
    const restored = keeper.restore(locate);
    const { scrollTop, scrollHeight, clientHeight } = element;
    if (restored !== undefined) {
      element.scrollTop = restored;
    } else if (keeper.follows(scrollTop, scrollHeight, clientHeight)) {
      element.scrollTop = scrollHeight;
    } else {
      return;
    }
    keeper.put(element.scrollTop);
  });

  const loadEarlier = () => {
    const element = scroller.current;
    const top = element?.scrollTop ?? 0;
    // The first row on screen that stays put: not the history control itself,
    // which is at the top before the page and after it.
    const first = virtualizer
      .getVirtualItems()
      .find((item) => item.end > top && rows[item.index]?.kind !== "history");
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
  const plan = runningPlan(rows);
  const toPlan = () => {
    const index = rows.findIndex((row) => row.key === plan?.key);
    keeper.stuck = false;
    setStuck(false);
    virtualizer.scrollToIndex(index, { align: "start" });
  };

  return (
    <StreamActions value={{ sessionId, loadEarlier }}>
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
        <ComposerOverlays plan={plan} onPlan={toPlan} away={!stuck} onBottom={toBottom} />
      </div>
    </StreamActions>
  );
}
