// The stream's other rows (design 5.7.4): thinking (MS-2), and notices and
// errors in the warning box (MS-5), their text from the semantic codes. The
// assistant's answer is `Markdown`; recovery steps are T143's.

import type { ErrorItem, Notice } from "@cupper/protocol";
import { escapeInvisible } from "@cupper/i18n";
import { useState } from "react";

import { useT, type Translate } from "../app/providers";
import { Alert } from "../components/Alert";

/** "12 秒", "1 分 5 秒". */
export function duration(t: Translate, ms: number): string {
  const s = Math.max(1, Math.round(ms / 1000));
  return s < 60
    ? t("stream:seconds", { count: s })
    : t("stream:minutes", { minutes: Math.floor(s / 60), seconds: s % 60 });
}

/** "正在思考" with a shimmer; then "已思考 {时长}", which opens on the thought. */
export function Thought(props: { text: string; running: boolean; durationMs?: number }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const label = props.running
    ? t("stream:thinking")
    : t("stream:thought", { duration: duration(t, props.durationMs ?? 0) });
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

export function NoticeRow({ notice }: { notice: Notice }) {
  const t = useT();
  const { code, params } = notice;
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

/** An error, and the agent's own text under it. */
export function ErrorRow({ error }: { error: ErrorItem }) {
  const t = useT();
  const key = ERRORS.includes(error.code) ? `stream:error_${error.code}` : "stream:error_other";
  return (
    <Alert tone="danger">
      <div>{t(key)}</div>
      {error.raw && (
        <div className="mt-1 break-words text-fg-tertiary">{escapeInvisible(error.raw)}</div>
      )}
    </Alert>
  );
}
