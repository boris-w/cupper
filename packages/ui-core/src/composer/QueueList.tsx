// Queued messages above the composer (design 5.7.4): a preview each, and the
// paused queue with "继续" (`session/queue_resume`). Steer and editing are
// M1b (T140, T149).

import type { SessionView } from "@cupper/host-client";
import type { PromptBlock } from "@cupper/protocol";

import { useT } from "../app/providers";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";
import { Composer } from "./Composer";
import { useSend } from "./send";

const count = (content: PromptBlock[], type: string) =>
  content.filter((block) => block.type === type).length;

export function QueueList({ sessionId }: { sessionId: string }) {
  const view = useStreams().session(sessionId)?.view;
  return view ? <Queue sessionId={sessionId} view={view} /> : null;
}

function Queue({ sessionId, view }: { sessionId: string; view: SessionView }) {
  const t = useT();
  const { resume } = useSend(sessionId, undefined);
  useEntry(view, "session");
  const queue = view.toJSON().queue;
  if (!queue || (queue.entries.length === 0 && !queue.paused)) {
    return null;
  }
  const reason = queue.paused?.reason;
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
        <div key={entry.queue_id} className={row}>
          <span className="min-w-0 flex-1 truncate">{entry.preview.split("\n", 1)[0]}</span>
          {(
            [
              ["image", "composer:queue_images"],
              ["review_comment", "composer:queue_comments"],
            ] as const
          ).map(([type, label]) => {
            const n = count(entry.content, type);
            return (
              n > 0 && (
                <span key={type} className="text-fg-tertiary">
                  {t(label, { count: n })}
                </span>
              )
            );
          })}
        </div>
      ))}
    </div>
  );
}

/** The session page's composer place: queued messages above the composer. */
export function ComposerArea({ sessionId }: { sessionId: string }) {
  return (
    <>
      <QueueList sessionId={sessionId} />
      <Composer sessionId={sessionId} />
    </>
  );
}
