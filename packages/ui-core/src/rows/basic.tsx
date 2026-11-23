// The rows T082 draws (design 5.7.4): "load earlier history", the turn
// header with its outcome, the user's message, and the placeholder of what
// this build cannot show; the time between turns and the footer under a
// turn's answer, as Codex has them (T272).

import { escapeInvisible, formatDivider, formatStamp } from "@cupper/i18n";
import type { BlobRef, PromptBlock } from "@cupper/protocol";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { useShell, useT } from "../app/providers";
import { ChevronDownIcon, CopyIcon } from "../components/icons";
import { useBlobText } from "../state/blob";
import { useStreamActions } from "../stream/actions";
import { UnsupportedRow } from "./registry";
import { registerRow, type RowOf } from "./stream";

/** A duration as the stream says it: seconds, or minutes and seconds. */
export function useDuration() {
  const t = useT();
  return (ms: number) => {
    const s = Math.floor(ms / 1000);
    return s < 60
      ? t("stream:duration_s", { s })
      : t("stream:duration_ms", { m: Math.floor(s / 60), s: s % 60 });
  };
}

function HistoryRow({ row }: { row: RowOf<"history"> }) {
  const t = useT();
  const { loadEarlier } = useStreamActions();
  return (
    <button
      type="button"
      onClick={loadEarlier}
      disabled={row.state === "loading"}
      className="mx-auto block text-sm text-fg-tertiary hover:text-fg-secondary"
    >
      {t(`stream:history_${row.state}`)}
    </button>
  );
}

/** Milliseconds since `since`, ticking every second while `live`. */
function useElapsed(since: number, live: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!live) {
      return undefined;
    }
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [live]);
  return Math.max(0, now - since);
}

/** "用时 {时长} ›" over a completed turn, which it folds (design 5.7.4); the others' outcome. */
function TurnHeaderRow({ row }: { row: RowOf<"turn_header"> }) {
  const t = useT();
  const { toggleTurn } = useStreamActions();
  const duration = useDuration();
  const running = row.status === "running";
  const elapsed = useElapsed(row.startedAt, running);
  const took = duration(row.durationMs ?? elapsed);
  const text =
    row.status === "cancelled"
      ? t("stream:turn_cancelled", { duration: took })
      : row.status === "interrupted" || row.status === "failed"
        ? t(`stream:turn_${row.status}`)
        : t(running ? "stream:turn_processed" : "stream:turn_took", { duration: took });
  const line =
    "flex w-full items-center gap-1 border-b border-border pb-1 text-sm text-fg-secondary";
  if (row.folded === undefined) {
    return <div className={line}>{text}</div>;
  }
  return (
    <button
      type="button"
      aria-expanded={!row.folded}
      onClick={() => toggleTurn(row.turnId)}
      className={`${line} text-left outline-none hover:text-fg focus-visible:ring-2 focus-visible:ring-ring`}
    >
      {text}
      <span
        className={`shrink-0 transition-transform duration-150 [&>svg]:size-3.5 ${row.folded ? "-rotate-90" : ""}`}
      >
        <ChevronDownIcon />
      </span>
    </button>
  );
}

function UserMessageRow({ row }: { row: RowOf<"user_message"> }) {
  const t = useT();
  const body = useRef<HTMLDivElement>(null);
  const [long, setLong] = useState(false);
  const [open, setOpen] = useState(false);
  useLayoutEffect(() => {
    const element = body.current;
    setLong(!!element && element.scrollHeight > element.clientHeight + 1);
  }, [row.content]);
  const text = row.content.map((block) => (block.type === "text" ? block.text : "")).join("\n");
  const comments = row.content.filter((block) => block.type === "review_comment").length;
  // A long text block downloads only on "显示更多"; until then it shows its size.
  const blobs = row.content.some((block) => block.type === "text_blob");
  return (
    <div className="group flex flex-col items-end">
      {row.replyTo && (
        <p className="pb-1 text-xs text-fg-tertiary">
          {row.replyTitle
            ? t("stream:reply_to", { title: escapeInvisible(row.replyTitle) })
            : t("stream:reply_to_other")}
        </p>
      )}
      <div className="max-w-[70%] rounded-bubble bg-user-bubble px-4 py-2.5 text-chat">
        {/* Folds past 12 lines. */}
        <div ref={body} className={open ? "" : "line-clamp-12"}>
          {row.content.map((block, n) => (
            <Block key={n} block={block} open={open} />
          ))}
          {comments > 0 && (
            <p className="text-fg-secondary">{t("stream:comments", { count: comments })}</p>
          )}
        </div>
        {(long || open || blobs) && (
          <button
            type="button"
            onClick={() => setOpen(!open)}
            className="mt-1 text-sm text-fg-tertiary"
          >
            {t(open ? "stream:show_less" : "stream:show_more")}
          </button>
        )}
      </div>
      <CopyButton
        text={text}
        className="mt-1 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
      />
    </div>
  );
}

/** Copies `text`; an icon, named "复制". */
function CopyButton({ text, className = "" }: { text: string; className?: string }) {
  const t = useT();
  return (
    <button
      type="button"
      aria-label={t("stream:copy")}
      onClick={() => void navigator.clipboard.writeText(text)}
      className={`rounded-md p-1 text-fg-tertiary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring ${className}`}
    >
      <CopyIcon />
    </button>
  );
}

/** "今天 14:05", centred, above a turn after a long gap. */
function TimeDividerRow({ row }: { row: RowOf<"time_divider"> }) {
  const { info } = useShell();
  const { day, time } = formatDivider(row.at, Date.now(), info.locale);
  return (
    <div role="separator" className="flex justify-center py-4 text-sm text-fg-tertiary select-none">
      <time dateTime={new Date(row.at).toISOString()}>
        <span className="font-medium">{day}</span> {time}
      </time>
    </div>
  );
}

/** Under a completed turn's answer: copy it, and when the turn ended; the last turn's always shows. */
function TurnFooterRow({ row }: { row: RowOf<"turn_footer"> }) {
  const { info } = useShell();
  const shown = row.last ? "" : "opacity-0 hover:opacity-100 focus-within:opacity-100";
  return (
    <div className={`flex items-center gap-2 text-sm text-fg-tertiary ${shown}`}>
      <CopyButton text={row.text} />
      <span>{formatStamp(row.endedAt, Date.now(), info.locale)}</span>
    </div>
  );
}

/** One block of a user's message, as text. */
function Block({ block, open }: { block: PromptBlock; open: boolean }) {
  const t = useT();
  switch (block.type) {
    case "text":
      return <p className="whitespace-pre-wrap">{String(block.text)}</p>;
    case "file_ref":
      return <p className="text-fg-secondary">@{String(block.path)}</p>;
    case "text_blob":
      return <TextBlob blob={block.blob as BlobRef} open={open} />;
    case "image":
      return <p className="text-fg-secondary">{t(`stream:${block.type}`)}</p>;
    default:
      return null;
  }
}

/** A text too long for one frame: its size, then itself once the bubble opens. */
function TextBlob({ blob, open }: { blob: BlobRef; open: boolean }) {
  const t = useT();
  const { status, text, load } = useBlobText(blob);
  useEffect(() => {
    if (open && status === "idle") load();
  }, [open, status, load]);
  if (text !== undefined) return <p className="whitespace-pre-wrap">{text}</p>;
  return (
    <p className="text-fg-secondary">
      {t("stream:text_blob_size", { kb: Math.ceil(blob.size / 1024) })}
      {status === "loading" && <span className="ml-2">{t("stream:text_blob_loading")}</span>}
      {status === "failed" && (
        <button type="button" onClick={load} className="ml-2 text-danger">
          {t("stream:text_blob_retry")}
        </button>
      )}
    </p>
  );
}

registerRow("history", HistoryRow);
registerRow("turn_header", TurnHeaderRow);
registerRow("user_message", UserMessageRow);
registerRow("time_divider", TimeDividerRow);
registerRow("turn_footer", TurnFooterRow);
registerRow("unknown", UnsupportedRow);
