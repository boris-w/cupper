// The stream's other rows (design 5.7.4): the assistant's answer in
// Markdown, thinking (MS-2), and notices and errors in the warning box
// (MS-5), their text from the semantic codes. Recovery steps are T143's.

import { escapeInvisible } from "@cupper/i18n";
import { useState } from "react";

import { useT } from "../app/providers";
import { Alert } from "../components/Alert";
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

/** An error, and the agent's own text under it. */
function ErrorRow({ row: { error } }: { row: RowOf<"error"> }) {
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

registerRow("agent_message", ({ row }) => <Markdown text={row.text} streaming={row.streaming} />);
registerRow("thought", ThoughtRow);
registerRow("notice", NoticeRow);
registerRow("error", ErrorRow);
