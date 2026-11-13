// Queued messages above the composer (design 5.7.4): a row each
// (`QueueRow`: steer, edit), and the paused queue with "继续"
// (`session/queue_resume`).

import type { SessionView } from "@cupper/host-client";
import type { QueueUpdated } from "@cupper/protocol";

import { useEffect, useRef } from "react";

import { useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import { RequestPanel, type ComposerActivity } from "../request/RequestPanel";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { Composer } from "./Composer";
import { QueueRow } from "./QueueRow";
import { useSend, useSessionState } from "./send";

function Queue(props: {
  sessionId: string;
  view: SessionView;
  queue: QueueUpdated | null | undefined;
  resume: () => void;
}) {
  const { queue, resume, view } = props;
  const t = useT();
  if (!queue || (queue.entries.length === 0 && !queue.paused)) {
    return null;
  }
  const reason = queue.paused?.reason;
  const { turns, capabilities } = view.toJSON();
  const running = turns.some((turn) => !turn.completed);
  const steer = capabilities?.turn.steer;
  const row = "flex items-center gap-2 rounded-lg bg-composer-bar px-3 py-1.5";
  return (
    <div
      data-queue-paused={reason !== undefined || undefined}
      className="flex flex-col gap-1 pb-2 text-sm"
    >
      {reason !== undefined && (
        <div className={`${row} text-fg-secondary`}>
          <span className="flex-1">
            {t(`composer:queue_paused_${reason}`, {
              defaultValue: t("composer:queue_paused_other"),
            })}
          </span>
          <button
            type="button"
            onClick={resume}
            className="rounded-md px-2 py-0.5 text-fg outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
          >
            {t("composer:resume")}
          </button>
        </div>
      )}
      {queue.entries.map((entry) => (
        <QueueRow
          key={entry.queue_id}
          sessionId={props.sessionId}
          entry={entry}
          version={queue.version}
          steerable={running && steer?.supported === true}
          sideEffects={steer?.side_effects === true}
        />
      ))}
    </div>
  );
}

/** The pending requests in creation order: the first takes the composer's place. */
const pending = (view: SessionView) =>
  Object.values(view.toJSON().requests)
    .filter((request) => !request.resolved)
    .toSorted((a, b) => a.requested.created_at - b.requested.created_at);

function SessionArea({ sessionId, view }: { sessionId: string; view: SessionView }) {
  const t = useT();
  const { show } = useToasts();
  const { resume } = useSend(sessionId, undefined);
  const { agent } = useSessionState(sessionId);
  const activity = useRef<ComposerActivity>({ focused: false, typedAt: 0 });
  const shown = useRef(new Set<string>());
  const answered = useRef(new Set<string>());
  // Any change: a new request lands in a turn or in the tail, and resolves under its own key.
  useEntry(view, "view");
  const waiting = pending(view);
  const first = waiting[0]?.requested;
  if (first) shown.current.add(first.request_id);

  // AP-4: a request shown here and answered by another client folds away with a toast.
  const { requests, queue } = view.toJSON();
  useEffect(() => {
    for (const id of shown.current) {
      const resolved = requests[id]?.resolved;
      if (!resolved) continue;
      shown.current.delete(id);
      const by = resolved.resolved_by?.client_name;
      if (by && resolved.status === "resolved" && !answered.current.has(id)) {
        show({ id: `elsewhere:${id}`, text: t("request:handled_elsewhere", { client: by }) });
      }
    }
  });

  const name = t(`sidebar:agent_${agent}`, { defaultValue: agent });
  return (
    <>
      <Queue sessionId={sessionId} view={view} queue={queue} resume={resume} />
      {first ? (
        <RequestPanel
          key={first.request_id}
          sessionId={sessionId}
          requested={first}
          agent={name}
          activity={activity}
          onAnswered={(id) => answered.current.add(id)}
          position={1}
          total={waiting.length}
        />
      ) : (
        <Composer sessionId={sessionId} paused={Boolean(queue?.paused)} activity={activity} />
      )}
    </>
  );
}

/** The session page's composer place: queued messages above the composer. */
export function ComposerArea({ sessionId }: { sessionId: string }) {
  const view = useStreams().session(sessionId)?.view;
  return view ? (
    <SessionArea sessionId={sessionId} view={view} />
  ) : (
    <Composer sessionId={sessionId} />
  );
}
