// A queued message's row above the composer (design 5.7.4): its preview,
// "插话" to steer it into the running turn now (`session/queue_edit`'s
// `steer`), and a menu to edit it in place (`replace`) or to switch
// "follow-ups queue by default". An edit made on a queue that changed
// since says so; the host's queue is what shows.

import type { PromptBlock, QueueEntry, QueueOp, SessionQueueEditResult } from "@cupper/protocol";
import { useRef, useState } from "react";

import { useHost, useShell, useT } from "../app/providers";
import { MoreIcon } from "../components/icons";
import { Menu, MenuItem } from "../components/Menu";
import { Tooltip } from "../components/Tooltip";
import { useToasts } from "../feedback/toaster";
import { composing, FOLLOW_UP_QUEUE_PREF } from "./keys";
import { codeOf, useHintOnce, usePref } from "./send";

const BUTTON =
  "rounded-md px-2 py-0.5 text-fg outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:text-fg-tertiary disabled:hover:bg-transparent";

const count = (content: PromptBlock[], type: string) =>
  content.filter((block) => block.type === type).length;

/** `session/queue_edit` at `version`; the result, or `undefined` once it says why not. */
function useQueueEdit(sessionId: string) {
  const { client } = useHost();
  const { show } = useToasts();
  const t = useT();
  return async (version: number, ops: QueueOp[]) => {
    const params = { session_id: sessionId, expected_version: version, ops };
    try {
      return await client.command<SessionQueueEditResult>("session/queue_edit", params);
    } catch (error) {
      const code = codeOf(error);
      const text =
        code === "conflict" ? t("composer:queue_changed") : t("composer:config_failed", { code });
      show({ id: "queue_edit", text });
      return undefined;
    }
  };
}

export function QueueRow(props: {
  sessionId: string;
  entry: QueueEntry;
  version: number;
  /** A turn runs and the agent steers. */
  steerable: boolean;
  sideEffects: boolean;
}) {
  const { sessionId, entry, version } = props;
  const t = useT();
  const { api } = useShell();
  const { show } = useToasts();
  const once = useHintOnce(sessionId);
  const edit = useQueueEdit(sessionId);
  const followUpQueue = usePref(FOLLOW_UP_QUEUE_PREF) === true;
  const [editing, setEditing] = useState(false);
  // Opened from the menu, the editor takes the focus the menu would give back.
  const opening = useRef(false);
  const editor = useRef<HTMLTextAreaElement>(null);
  const [busy, setBusy] = useState(false);
  const queueId = entry.queue_id;

  const steer = async () => {
    if (props.sideEffects) once("steer_side_effects");
    setBusy(true);
    const edited = await edit(version, [{ op: "steer", queue_id: queueId }]);
    setBusy(false);
    if (edited?.steer?.fallback_reason) show({ id: "steer", text: t("composer:steer_failed") });
  };
  /** The message with `text` for its text blocks; images and comments stay. */
  const replace = async (text: string) => {
    const kept = entry.content.filter((block) => block.type !== "text");
    const content = [...(text.trim() ? [{ type: "text" as const, text }] : []), ...kept];
    if (content.length > 0) await edit(version, [{ op: "replace", queue_id: queueId, content }]);
    opening.current = false;
    setEditing(false);
  };

  return (
    <div
      data-queue-id={queueId}
      className="flex items-center gap-2 rounded-lg bg-composer-bar px-3 py-1.5"
    >
      {editing ? (
        <textarea
          ref={editor}
          rows={1}
          aria-label={t("composer:queue_edit")}
          defaultValue={entry.content
            .flatMap((b) => (b.type === "text" ? [b.text] : []))
            .join("\n")}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              opening.current = false;
              setEditing(false);
            }
            if (event.key !== "Enter" || event.shiftKey || composing(event.nativeEvent)) return;
            event.preventDefault();
            void replace(event.currentTarget.value);
          }}
          className="field-sizing-content min-w-0 flex-1 resize-none bg-transparent outline-none"
        />
      ) : (
        <span className="min-w-0 flex-1 truncate">{entry.preview.split("\n", 1)[0]}</span>
      )}
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
      <Tooltip content={t("composer:steer_tip")}>
        <button
          type="button"
          disabled={!props.steerable || busy}
          onClick={() => void steer()}
          className={BUTTON}
        >
          {t("composer:steer")}
        </button>
      </Tooltip>
      <Menu
        onCloseAutoFocus={(event) => {
          if (!opening.current) return;
          event.preventDefault();
          editor.current?.focus();
        }}
        trigger={
          <button type="button" aria-label={t("composer:queue_menu")} className={BUTTON}>
            <MoreIcon />
          </button>
        }
      >
        <MenuItem
          onSelect={() => {
            opening.current = true;
            setEditing(true);
          }}
        >
          {t("composer:queue_edit")}
        </MenuItem>
        <MenuItem onSelect={() => void api.prefs.set(FOLLOW_UP_QUEUE_PREF, !followUpQueue)}>
          {t(followUpQueue ? "composer:queue_off" : "composer:queue_on")}
        </MenuItem>
      </Menu>
    </div>
  );
}
