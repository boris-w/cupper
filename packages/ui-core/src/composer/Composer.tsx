// The composer (design 5.7.4, ui-spec 4.5): attachments, the text area and
// the bottom bar with the send button. The draft and the attachments live in
// the UI store per session, so the request panel (T083) can take the
// composer's place and give it back as it was.

import type { PromptBlock } from "@cupper/protocol";
import { useEffect, useRef, useState } from "react";
import { useStore } from "zustand";

import { useT } from "../app/providers";
import { useRouter } from "../app/router";
import { ArrowUpIcon, CloseIcon, StopIcon } from "../components/icons";
import { useUi } from "../state/provider";
import { EXTENSIONS, OverlayContext } from "./extensions";
import { escapeAction, SEND_KEY_PREF, sends } from "./keys";
import { NEW_DRAFT, usePref, useSend, useSessionState } from "./send";

const NONE: readonly PromptBlock[] = [];

function Attachments(props: { blocks: readonly PromptBlock[]; remove: (index: number) => void }) {
  const t = useT();
  const label = (block: PromptBlock) => {
    switch (block.type) {
      case "image":
        return t("composer:attachment_image");
      case "file_ref":
        return "path" in block ? String(block.path) : t("composer:attachment_other");
      case "review_comment":
        return "path" in block
          ? t("composer:attachment_comment", { path: block.path, line: block.line_start })
          : t("composer:attachment_other");
      default:
        return t("composer:attachment_other");
    }
  };
  return (
    <ul className="flex flex-wrap gap-1.5 px-3 pt-3">
      {props.blocks.map((block, index) => (
        // Attachments have no id; their order is stable while shown.
        <li
          key={index}
          className="flex items-center gap-1 rounded-lg border border-border py-0.5 pr-0.5 pl-2 text-sm"
        >
          {label(block)}
          <button
            type="button"
            aria-label={t("composer:remove")}
            onClick={() => props.remove(index)}
            className="rounded-md text-fg-tertiary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
          >
            <CloseIcon />
          </button>
        </li>
      ))}
    </ul>
  );
}

/** The composer of a session, or of the new session page when `sessionId` is absent. */
export function Composer({ sessionId }: { sessionId?: string }) {
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
  const { send, interrupt } = useSend(sessionId, projectId);
  const sendKey = usePref(SEND_KEY_PREF) === "mod_enter" ? "mod_enter" : "enter";
  const [armed, setArmed] = useState(false);
  const [container, setContainer] = useState<HTMLElement | null>(null);
  const closeOverlay = useRef<(() => void) | null>(null);
  const area = useRef<HTMLTextAreaElement>(null);

  const ready = sessionId !== undefined || projectId !== undefined;
  const empty = draft.trim() === "" && attachments.length === 0;
  const state = empty ? (running ? "stop" : "disabled") : ready ? "send" : "disabled";
  const submit = () => state === "send" && void send(draft, attachments);

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
            if (event.key !== "Escape") {
              setArmed(false);
              if (sends(event.nativeEvent, sendKey)) {
                event.preventDefault();
                submit();
              }
              return;
            }
            const action = escapeAction({
              focused: true,
              overlayOpen: closeOverlay.current !== null,
              running,
              armed,
            });
            if (action === "close_overlay") {
              closeOverlay.current?.();
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
