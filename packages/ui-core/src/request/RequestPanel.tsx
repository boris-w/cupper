// The request panel (design 5.7.4, 5.7.8 "请求面板的落点"): it takes the
// composer's place while a request waits. The rules that keep typing and a
// click meant for the composer off the panel live here: arming (RP-1, RP-2,
// RP-8), keyboard activation (RP-3), and no keys under a popover (RP-9).

import type { InteractionOption, InteractionOutcome, InteractionRequested } from "@cupper/protocol";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MutableRefObject,
} from "react";

import { escapeInvisible } from "@cupper/i18n";

import { useHost, useT } from "../app/providers";
import { useToasts } from "../feedback/toaster";
import { usePrivileged } from "../feedback/privileged";
import { useUi } from "../state/provider";
import { ContentBox } from "./fulltext";
import { BUTTON, OptionsKind, QuestionKind, text, type KindProps } from "./kinds";
import { handled, panelKey, skipOutcome, type Handled, type PanelKey } from "./logic";
import { useRespond } from "./respond";

/** RP-1: the panel takes no key or click for this long, counted again from each key. */
export const ARMING_MS = 500;
/** RP-3: typing in the composer this recently keeps the panel from taking the focus. */
export const TYPED_RECENTLY_MS = 2000;

/** What the composer tells the panel that replaces it: focus, and when it was last typed in. */
export interface ComposerActivity {
  focused: boolean;
  typedAt: number;
}

type Hint = "manual" | "cmd_enter" | "again" | null;

const TITLES: Partial<Record<string, string>> = { execute: "execute", edit: "edit" };

export function RequestPanel(props: {
  sessionId: string;
  requested: InteractionRequested;
  agent: string;
  activity: MutableRefObject<ComposerActivity>;
  onAnswered: (requestId: string) => void;
  /** Its place among the pending requests, for "1 / 3". */
  position: number;
  total: number;
}) {
  const { sessionId, requested } = props;
  const request = requested.request;
  const t = useT();
  const ui = useUi();
  const respond = useRespond(sessionId);
  const privileged = usePrivileged();
  const [ready, setReady] = useState(false);
  const [active, setActive] = useState(false);
  const [hint, setHint] = useState<Hint>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const downWhileReady = useRef(false);
  const cmdEnterSeen = useRef(false);
  const keys = useRef<(key: PanelKey) => void>(() => undefined);
  const list = useRef<HTMLUListElement>(null);

  const arm = useCallback(() => {
    setReady(false);
    clearTimeout(timer.current);
    timer.current = setTimeout(() => setReady(true), ARMING_MS);
  }, []);
  useEffect(() => {
    arm();
    return () => clearTimeout(timer.current);
  }, [arm]);

  const activate = (clearHint = true) => {
    setActive(true);
    if (clearHint) setHint(null);
    cmdEnterSeen.current = false;
  };
  const revoke = () => {
    setActive(false);
    setHint("manual");
  };

  // RP-3: the focus moves to the panel only when it was in the composer, the
  // composer is empty and was not typed in for 2 s; it never leaves elsewhere.
  useEffect(() => {
    const { drafts, attachments } = ui.getState();
    const empty = !drafts[sessionId] && (attachments[sessionId]?.length ?? 0) === 0;
    const { focused, typedAt } = props.activity.current;
    if (focused && empty && Date.now() - typedAt >= TYPED_RECENTLY_MS) {
      list.current?.focus();
      activate();
    } else {
      setHint("manual");
    }
    // Decided once, when the panel appears.
  }, []);

  const { client } = useHost();
  const { show } = useToasts();
  const answer = (outcome: InteractionOutcome) =>
    void respond(requested, outcome).then((how) => {
      if (how === "sent") props.onAnswered(requested.request_id);
      if (how === "again") setHint("again");
      if (how === "revoke") revoke();
    });
  const confirm = (option: InteractionOption) =>
    void privileged({
      kind: "interaction",
      session_id: sessionId,
      request_id: requested.request_id,
      option_id: option.option_id,
    }).then((result) => result.status === "confirmed" && props.onAnswered(requested.request_id));
  const close =
    request.type === "question" || !("options" in request)
      ? { type: "dismissed" as const }
      : skipOutcome(request);
  const click = (fn: () => void) => () => downWhileReady.current && fn();
  // RP-12: the rejection first; the follow-up only once it went, queued and
  // tied to the request. Otherwise the text goes back to the composer, after
  // a draft already there with a blank line between.
  const keepInComposer = (message: string) => {
    const draft = ui.getState().drafts[sessionId];
    ui.getState().setDraft(sessionId, draft ? `${draft}\n\n${message}` : message);
  };
  const rejectWithMessage = async (message: string, reject: InteractionOutcome) => {
    const how = await respond(requested, reject);
    if (how === "sent") {
      props.onAnswered(requested.request_id);
      const content = [{ type: "text", text: message }];
      const params = {
        session_id: sessionId,
        mode: "queue",
        content,
        reply_to: requested.request_id,
      };
      await client.command("session/send", params).catch(() => {
        keepInComposer(message);
        show({ id: "follow-up", text: t("composer:send_failed", { code: "follow_up" }) });
      });
    } else if (how === "elsewhere") {
      keepInComposer(message);
    } else if (how !== "failed") {
      setHint(how === "again" ? "again" : "manual");
      if (how === "revoke") revoke();
    }
  };
  const kind: KindProps = {
    requested,
    agent: props.agent,
    answer,
    confirm,
    click,
    keys,
    listRef: list,
  };

  const onKeyDown = (event: KeyboardEvent) => {
    if (!ready) {
      event.preventDefault();
      arm();
      return;
    }
    if (document.querySelector("[role=menu], [role=dialog]")) {
      return;
    }
    const key = panelKey(event.nativeEvent);
    const field = event.target instanceof HTMLInputElement ? event.target : null;
    if (field) {
      // Typing in a field is input, not a typing key. Esc leaves a filled field first.
      if (key.type === "close") {
        event.preventDefault();
        return field.value ? list.current?.focus() : answer(close);
      }
      return;
    }
    if (!active) {
      if (key.type !== "tab") event.preventDefault();
      return;
    }
    if (key.type === "tab" || key.type === "other") {
      return;
    }
    event.preventDefault();
    if (key.type === "typing") return revoke();
    if (key.type === "close") return answer(close);
    if (key.type === "cmd_submit" && !cmdEnterSeen.current) {
      cmdEnterSeen.current = true;
      return setHint("cmd_enter");
    }
    keys.current(key);
  };

  const title = !handled(request)
    ? t("request:unsupported")
    : request.type === "permission"
      ? t(`request:title_${TITLES[request.tool.tool_kind] ?? "other"}`, { agent: props.agent })
      : request.type === "plan_confirm"
        ? t("request:title_plan")
        : escapeInvisible(request.prompt);

  return (
    <div
      data-request-panel=""
      data-request-kind={request.type}
      data-ready={ready || undefined}
      data-activation={active ? "auto" : "manual"}
      onKeyDown={onKeyDown}
      onFocus={() => !active && ready && activate()}
      onMouseDownCapture={() => {
        downWhileReady.current = ready;
        // The hint goes once the click is over: taking it away now would move
        // the buttons under the pointer, and the click would miss (RP-2).
        if (ready) activate(false);
      }}
      onClickCapture={() => downWhileReady.current && setHint(null)}
      className="rounded-composer bg-surface-composer shadow-composer"
    >
      <div className="flex items-start gap-2 px-4 pt-3 pb-2">
        <p className="min-w-0 flex-1 text-chat font-medium">{title}</p>
        {props.total > 1 && (
          <span data-request-count="" className="pt-0.5 text-sm text-fg-tertiary">
            {t("request:count", { position: props.position, total: props.total })}
          </span>
        )}
        <button
          type="button"
          onClick={click(() => answer(close))}
          className={`${BUTTON} shrink-0 text-sm text-fg-tertiary`}
        >
          {t("request:close")}
          <span className="ml-1.5 rounded-xs border border-border px-1 text-xs">
            {t("request:key_esc")}
          </span>
        </button>
      </div>
      <Body
        {...kind}
        rejectWithMessage={(message, reject) => void rejectWithMessage(message, reject)}
      />
      {hint && (
        <p role="status" className="px-4 pb-3 text-sm text-fg-tertiary">
          {t(
            hint === "again"
              ? "request:press_again"
              : hint === "cmd_enter"
                ? "request:cmd_enter"
                : "request:manual",
          )}
        </p>
      )}
    </div>
  );
}

/** The kind's content, options and buttons; a kind this build does not handle only closes. */
function Body(
  props: KindProps & { rejectWithMessage: (text: string, reject: InteractionOutcome) => void },
) {
  const t = useT();
  const { request } = props.requested;
  if (!handled(request)) {
    props.keys.current = () => undefined;
    return null;
  }
  switch (request.type) {
    case "permission":
      return <PermissionBody {...props} request={request} />;
    case "plan_confirm":
      return (
        <OptionsKind
          {...props}
          options={request.options}
          truncated={false}
          skip={skipOutcome(request)}
          rejectWithMessage={undefined}
        >
          <p className="mx-4 mb-2 max-h-60 overflow-y-auto text-sm whitespace-pre-wrap">
            {text(request.plan)}
          </p>
        </OptionsKind>
      );
    case "question":
      if (!request.unsupported_schema) {
        return <QuestionKind {...props} fields={request.fields} />;
      }
      props.keys.current = () => undefined;
      return (
        <p className="px-4 pb-3 text-sm text-fg-tertiary">{t("request:unsupported_schema")}</p>
      );
    default:
      props.keys.current = () => undefined;
      return null;
  }
}

/**
 * A permission: its title, command, paths and details. Truncated content
 * keeps the allowing options off until every blob in it is expanded and
 * loaded (security #6); collapsing one or a failed download takes them back.
 */
function PermissionBody(
  props: Parameters<typeof Body>[0] & { request: Extract<Handled, { type: "permission" }> },
) {
  const t = useT();
  const { request } = props;
  const reject = skipOutcome(request);
  const [seen, setSeen] = useState({ command: false, detail: false });
  const seeCommand = useCallback(
    (now: boolean) => setSeen((was) => ({ ...was, command: now })),
    [],
  );
  const seeDetail = useCallback((now: boolean) => setSeen((was) => ({ ...was, detail: now })), []);
  const { command, paths } = request.tool;
  const blobs = { command: command?.type === "blob", detail: request.detail?.type === "blob" };
  const whole =
    (blobs.command || blobs.detail) &&
    (!blobs.command || seen.command) &&
    (!blobs.detail || seen.detail);
  return (
    <OptionsKind
      {...props}
      options={request.options}
      truncated={request.content_truncated && !whole}
      skip={reject}
      rejectWithMessage={
        reject.type === "selected"
          ? (message) => props.rejectWithMessage(message, reject)
          : undefined
      }
    >
      {request.title && (
        <p className="px-4 pb-2 text-sm text-fg-secondary">{escapeInvisible(request.title)}</p>
      )}
      {command && (
        <ContentBox label={t("request:terminal")} content={command} onSeen={seeCommand} />
      )}
      {paths.length > 0 && (
        <ul className="px-4 pb-2 font-mono text-code">
          {paths.map((path) => (
            <li key={path}>{escapeInvisible(path)}</li>
          ))}
        </ul>
      )}
      {request.detail?.type === "blob" ? (
        <ContentBox label={t("request:detail")} content={request.detail} onSeen={seeDetail} />
      ) : (
        text(request.detail) && (
          <p className="px-4 pb-2 text-sm whitespace-pre-wrap text-fg-secondary">
            {text(request.detail)}
          </p>
        )
      )}
    </OptionsKind>
  );
}
