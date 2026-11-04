// The stream's other rows (design 5.7.4): the assistant's answer in
// Markdown, thinking (MS-2), and notices and errors in the warning box
// (MS-5), their text from the semantic codes, an error's recovery step in it.

import { escapeInvisible } from "@cupper/i18n";
import type { PromptBlock } from "@cupper/protocol";
import { useState } from "react";

import { useHost, useT } from "../app/providers";
import { Alert } from "../components/Alert";
import { codeOf, useSessionState } from "../composer/send";
import { useToasts } from "../feedback/toaster";
import { useUi } from "../state/provider";
import { useStreamActions } from "../stream/actions";
import { useDuration } from "./basic";
import { Markdown } from "./markdown";
import { registerRow, type RowOf } from "./stream";

/** "正在思考" with a shimmer; then "已思考 {时长}", which opens on the thought. */
function ThoughtRow({ row: props }: { row: RowOf<"thought"> }) {
  const t = useT();
  const duration = useDuration();
  const [open, setOpen] = useState(false);
  const label = props.running
    ? t("stream:thinking")
    : t("stream:thought", { duration: duration(props.durationMs ?? 0) });
  const expandable = !props.running && props.text !== "";
  return (
    <div className="text-chat-sm text-fg-tertiary">
      <button
        type="button"
        disabled={!expandable}
        aria-expanded={expandable ? open : undefined}
        onClick={() => setOpen(!open)}
        className={props.running ? "shimmer" : ""}
      >
        {label}
      </button>
      {open && <div className="mt-1 whitespace-pre-wrap">{escapeInvisible(props.text)}</div>}
    </div>
  );
}

const NOTICES = [
  "agent_restarted",
  "session_resumed",
  "compaction",
  "upstream_retry",
  "snapshot_unavailable",
  "repo_config_changed",
  "unsafe_repo_config",
  "project_config_changed",
  "config_unconfirmed",
];

function NoticeRow({ row }: { row: RowOf<"notice"> }) {
  const t = useT();
  const { code, params } = row.notice;
  const key = NOTICES.includes(code) ? `stream:notice_${code}` : "stream:notice_other";
  return <Alert>{t(key, { code, attempt: params.attempt, max: params.max })}</Alert>;
}

const ERRORS = [
  "not_logged_in",
  "cli_too_old",
  "session_not_found",
  "process_exited",
  "restartable",
  "model_unsupported",
  "quota_exhausted",
  "adapter_exited",
  "adapter_crashed",
  "cli_missing",
  "runtime_failed",
  "host_restarted",
];

/** Recovery steps with a text; `none`, and a step this build does not know, add nothing. */
const GUIDANCE = [
  "retry",
  "run_login_in_terminal",
  "upgrade_cli",
  "install_cli",
  "quota_exhausted",
  "change_model",
  "restart_agent",
];

/** What logs an agent in: cupper does not, the user does in a terminal (AG-2). */
const LOGIN: Record<string, string> = { claude: "claude", codex: "codex login" };

const BUTTON =
  "shrink-0 rounded-md bg-btn-secondary px-2 py-0.5 text-fg hover:bg-hover disabled:opacity-50";

/** Sends the turn's first message again, as it was: text, files, pictures, comments. */
function Retry({ content }: { content: PromptBlock[] }) {
  const t = useT();
  const { client } = useHost();
  const { show } = useToasts();
  const { sessionId } = useStreamActions();
  const [sending, setSending] = useState(false);
  const retry = () => {
    setSending(true);
    client
      .command("session/send", { session_id: sessionId, mode: "queue", content })
      .catch((error: unknown) =>
        show({ id: "send", text: t("composer:send_failed", { code: codeOf(error) }) }),
      )
      .finally(() => setSending(false));
  };
  return (
    <button type="button" disabled={sending} onClick={retry} className={BUTTON}>
      {t("stream:retry")}
    </button>
  );
}

/** The command that logs the session's agent in, to copy into a terminal. */
function LoginCommand() {
  const t = useT();
  const { agent } = useSessionState(useStreamActions().sessionId);
  const command = LOGIN[agent];
  return (
    command && (
      <span className="mt-1 flex items-center gap-2">
        <code className="rounded-xs bg-btn-secondary px-1 font-mono text-code">{command}</code>
        <button
          type="button"
          onClick={() => void navigator.clipboard.writeText(command)}
          className={BUTTON}
        >
          {t("stream:copy_command")}
        </button>
      </span>
    )
  );
}

/** "换模型": opens the composer's model menu (T143's entry). */
function ChangeModel() {
  const t = useT();
  const ui = useUi();
  return (
    <button type="button" onClick={() => ui.getState().openMenu("model")} className={BUTTON}>
      {t("stream:change_model")}
    </button>
  );
}

/** An error, the agent's own text under it, and what to do about it. */
function ErrorRow({ row: { error, retry } }: { row: RowOf<"error"> }) {
  const t = useT();
  const key = ERRORS.includes(error.code) ? `stream:error_${error.code}` : "stream:error_other";
  const guidance = GUIDANCE.includes(error.guidance) ? error.guidance : undefined;
  const button = guidance === "retry" && retry;
  return (
    <Alert
      tone="danger"
      action={
        (button && <Retry content={button} />) || (guidance === "change_model" && <ChangeModel />)
      }
    >
      <div>{t(key)}</div>
      {error.raw && (
        <div className="mt-1 break-words text-fg-tertiary">{escapeInvisible(error.raw)}</div>
      )}
      {guidance && !button && (
        <div data-guidance={guidance} className="mt-1 text-fg-secondary">
          {t(`stream:guidance_${guidance}`)}
          {guidance === "run_login_in_terminal" && <LoginCommand />}
        </div>
      )}
    </Alert>
  );
}

registerRow("agent_message", ({ row }) => <Markdown text={row.text} streaming={row.streaming} />);
registerRow("thought", ThoughtRow);
registerRow("notice", NoticeRow);
registerRow("error", ErrorRow);
