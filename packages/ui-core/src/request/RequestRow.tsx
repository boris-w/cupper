// A request's place in the message stream (design 5.7.4): one line on how it
// ended, "已批准：…", "已拒绝", "已询问 n 个问题" or "未回答"; while it waits,
// the panel has it and the line says so.

import { escapeInvisible } from "@cupper/i18n";
import type { InteractionOption, RequestView } from "@cupper/protocol";

import { useT } from "../app/providers";
import { registerRow, type RowOf } from "../rows/stream";

interface Asked {
  title?: unknown;
  fields?: unknown[];
  options?: InteractionOption[];
}

/** The record's text key and values, from the request and its resolution. */
export function record(view: RequestView): [string, Record<string, unknown>?] {
  const { resolved } = view;
  const asked = view.requested.request as Asked;
  const outcome = resolved?.outcome;
  if (!resolved || resolved.status === "pending") {
    return ["request:record_pending"];
  }
  if (outcome?.type === "answer") {
    return ["request:record_asked", { count: asked.fields?.length ?? 0 }];
  }
  if (outcome?.type === "selected" && "option_id" in outcome) {
    const option = asked.options?.find((o) => o.option_id === outcome.option_id);
    const rejected = option?.effect.scope === "reject" || option?.effect.scope === "plan_revise";
    if (rejected) {
      return ["request:record_rejected"];
    }
    const title =
      typeof asked.title === "string" ? escapeInvisible(asked.title) : (option?.name ?? "");
    return ["request:record_approved", { title }];
  }
  return ["request:record_unanswered"];
}

function RequestRow({ row }: { row: RowOf<"request"> }) {
  const t = useT();
  if (!row.request) {
    return null;
  }
  const [key, values] = record(row.request);
  return (
    <p data-request-record={row.requestId} className="text-sm text-fg-tertiary">
      {t(key, values)}
    </p>
  );
}

registerRow("request", RequestRow);
