// The composer (design 5.7.4, ui-spec 4.5): attachments, the text area and
// the bottom bar with the send button. The draft and the attachments live in
// the UI store per session, so the request panel (T083) can take the
// composer's place and give it back as it was.

import type { PromptBlock } from "@cupper/protocol";
import { useEffect, useLayoutEffect, useRef, useState, type MutableRefObject } from "react";
import { useStore } from "zustand";

import { useT } from "../app/providers";
import { useRouter } from "../app/router";
import { Alert } from "../components/Alert";
import { ArrowUpIcon, CloseIcon, StopIcon } from "../components/icons";
import type { ComposerActivity } from "../request/RequestPanel";
import { isComment } from "../review/view";
import { useUi } from "../state/provider";
import { EXTENSIONS, OverlayContext } from "./extensions";
import { escapeAction, SEND_KEY_PREF, sends } from "./keys";
import { NEW_DRAFT, usePref, useSend, useSessionState, type NewSession } from "./send";

const NONE: readonly PromptBlock[] = [];
const ACTION =
  "rounded-lg bg-btn-secondary px-2.5 py-1 outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring";

/**
 * The attachment bar: a chip per image or file, and the review comments
 * together as "{n} 条评论", which opens to the file, line and text of each,
 * each removable (design 5.7.4, GT-2).
 */
function Attachments(props: { blocks: readonly PromptBlock[]; remove: (index: number) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const label = (block: PromptBlock) => {
    switch (block.type) {
      case "image":
        return t("composer:attachment_image");
      case "file_ref":
        return "path" in block ? String(block.path) : t("composer:attachment_other");
      default:
        return t("composer:attachment_other");
    }
  };
  // Attachments have no id; their order is stable while shown.
  const indexed = props.blocks.map((block, index) => ({ block, index }));
  const comments = indexed.flatMap(({ block, index }) =>
    isComment(block) ? [{ comment: block, index }] : [],
  );
  const remove = (index: number) => (
    <button
      type="button"
      aria-label={t("composer:remove")}
      onClick={() => props.remove(index)}
      className="rounded-md text-fg-tertiary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
    >
      <CloseIcon />
    </button>
  );
  return (
    <div className="px-3 pt-3">
      <ul className="flex flex-wrap gap-1.5">
        {indexed
          .filter(({ block }) => block.type !== "review_comment")
          .map(({ block, index }) => (
            <li key={index} className={CHIP}>
              {label(block)}
              {remove(index)}
            </li>
          ))}
        {comments.length > 0 && (
          <li className={CHIP}>
            <button
              type="button"
              aria-expanded={open}
              onClick={() => setOpen(!open)}
              className="rounded-md outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
              {t("composer:attachment_comments", { count: comments.length })}
            </button>
          </li>
        )}
      </ul>
      {open && comments.length > 0 && (
        <ul className="mt-1.5 flex flex-col gap-1 text-sm">
          {comments.map(({ comment, index }) => (
            <li key={index} className="flex items-start gap-2">
              <span className="shrink-0 text-fg-tertiary">
                {t("composer:attachment_comment_at", {
                  path: comment.path,
                  line: comment.line_start,
                })}
              </span>
              <span className="min-w-0 flex-1 break-words">{comment.comment}</span>
              {remove(index)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

const CHIP = "flex items-center gap-1 rounded-lg border border-border py-0.5 pr-0.5 pl-2 text-sm";

/** The composer of a session, or of the new session page when `sessionId` is absent. */
export function Composer(props: {
  sessionId?: string;
  paused?: boolean;
  /** Where the composer reports focus and typing, for the request panel that may replace it (RP-3). */
  activity?: MutableRefObject<ComposerActivity>;
  /** On the new session page: the agent and workspace the session is created with. */
  create?: NewSession;
}) {
  const { sessionId, paused = false, activity, create } = props;
  const t = useT();
  const ui = useUi();
  const { route } = useRouter();
  const key = sessionId ?? NEW_DRAFT;
  const draft = useStore(ui, (state) => state.drafts[key] ?? "");
  const attachments = useStore(ui, (state) => state.attachments[key] ?? NONE);
  const { agent, running, firstProject } = useSessionState(sessionId);
  const projectId = sessionId
    ? undefined
    : (route.page === "new" && route.projectId) || firstProject;
  const { send, interrupt, resume } = useSend(sessionId, projectId, create);
  const sendKey = usePref(SEND_KEY_PREF) === "mod_enter" ? "mod_enter" : "enter";
  const [armed, setArmed] = useState(false);
  /** Sending while the queue is paused asks first (design 5.7.4): send and resume, or cancel. */
  const [confirming, setConfirming] = useState(false);
  const [container, setContainer] = useState<HTMLElement | null>(null);
  const closeOverlay = useRef<(() => void) | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  const ready = sessionId !== undefined || projectId !== undefined;
  const empty = draft.trim() === "" && attachments.length === 0;
  const state = empty ? (running ? "stop" : "disabled") : ready ? "send" : "disabled";
  const submit = () => {
    if (state === "send") {
      return paused ? setConfirming(true) : void send(draft, attachments);
    }
  };
  const confirmSend = async () => {
    setConfirming(false);
    if (await send(draft, attachments)) {
      resume();
    }
  };
  const cancel = () => {
    setConfirming(false);
    area.current?.focus();
  };

  // RP-3 reads where the page's focus was as the panel takes this place, the
  // window in front or not; a panel that had it gives it back.
  useLayoutEffect(() => {
    const node = area.current;
    if (activity?.current.focused) node?.focus();
    return () => {
      if (activity) activity.current.focused = document.activeElement === node;
    };
  }, [activity]);

  // Esc from outside the composer (and outside other fields, menus and dialogs) brings the focus back.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      const busy = document.activeElement?.closest("input, textarea, [role=menu], [role=dialog]");
      if (
        event.key === "Escape" &&
        !busy &&
        escapeAction({ focused: false, overlayOpen: false, running, armed }) === "focus"
      ) {
        area.current?.focus();
      }
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [running, armed]);

  const placeholder = !sessionId
    ? t(ready ? "composer:placeholder_new" : "composer:no_project")
    : running
      ? t("composer:placeholder_running")
      : t("composer:placeholder", { agent: t(`sidebar:agent_${agent}`, { defaultValue: agent }) });

  return (
    <OverlayContext value={{ container, setClose: (close) => (closeOverlay.current = close) }}>
      <div ref={setContainer} className="relative" />
      {armed && (
        <p role="status" className="pb-2 text-center text-sm text-fg-tertiary">
          {t("composer:esc_hint")}
        </p>
      )}
      {confirming && (
        <div className="pb-2" onKeyDown={(event) => event.key === "Escape" && cancel()}>
          <Alert
            action={
              <span className="flex shrink-0 gap-1">
                <button
                  type="button"
                  autoFocus
                  onClick={() => void confirmSend()}
                  className={ACTION}
                >
                  {t("composer:send")}
                </button>
                <button type="button" onClick={cancel} className={ACTION}>
                  {t("composer:cancel")}
                </button>
              </span>
            }
          >
            {t("composer:paused_send")}
          </Alert>
        </div>
      )}
      <div
        data-composer=""
        data-esc-armed={armed || undefined}
        className="rounded-composer bg-surface-composer shadow-composer"
      >
        {attachments.length > 0 && (
          <Attachments
            blocks={attachments}
            remove={(index) => ui.getState().setAttachments(key, attachments.toSpliced(index, 1))}
          />
        )}
        <textarea
          ref={area}
          rows={1}
          value={draft}
          placeholder={placeholder}
          aria-label={placeholder}
          onChange={(event) => ui.getState().setDraft(key, event.target.value)}
          onBlur={() => setArmed(false)}
          onKeyDown={(event) => {
            if (activity) activity.current.typedAt = Date.now();
            if (event.key !== "Escape") {
              setArmed(false);
              if (sends(event.nativeEvent, sendKey)) {
                event.preventDefault();
                submit();
              }
              return;
            }
            const overlayOpen = confirming || closeOverlay.current !== null;
            const action = escapeAction({ focused: true, overlayOpen, running, armed });
            if (action === "close_overlay") {
              return confirming ? setConfirming(false) : closeOverlay.current?.();
            }
            setArmed(action === "arm");
            if (action === "interrupt") {
              interrupt();
            }
          }}
          className="field-sizing-content block max-h-[25dvh] min-h-10 w-full resize-none bg-transparent px-3 pt-3 text-chat outline-none placeholder:text-fg/50"
        />
        <div className="flex items-center gap-1 p-2 text-sm">
          {EXTENSIONS.filter((item) => item.side === "left").map(({ Component }, index) => (
            <Component key={index} sessionId={sessionId} />
          ))}
          <div className="flex-1" />
          {EXTENSIONS.filter((item) => item.side === "right").map(({ Component }, index) => (
            <Component key={index} sessionId={sessionId} />
          ))}
          <button
            type="button"
            data-send-state={state}
            aria-label={t(state === "stop" ? "composer:stop" : "composer:send")}
            disabled={state === "disabled"}
            onClick={state === "stop" ? interrupt : submit}
            className="flex size-7 items-center justify-center rounded-full bg-btn-primary text-btn-primary-fg outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
          >
            {state === "stop" ? <StopIcon /> : <ArrowUpIcon />}
          </button>
        </div>
      </div>
    </OverlayContext>
  );
}
