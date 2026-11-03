// The agent's plan (design 5.7.4, MS-4): numbered steps, each with a status
// icon; or, for a plan in Markdown only, a "计划" bar that opens on it.

import { escapeInvisible } from "@cupper/i18n";
import type { BlobRef, Content, PlanEntry } from "@cupper/protocol";
import { useState } from "react";

import { useT } from "../app/providers";
import { ChevronDownIcon, StepIcon } from "../components/icons";
import { useBlobText } from "../state/blob";
import { Markdown } from "./markdown";
import { registerRow, type RowOf } from "./stream";

/** A status this build does not know shows as not started. */
const status = (entry: PlanEntry) =>
  (["in_progress", "completed"] as const).find((known) => known === entry.status) ?? "pending";

function Steps({ entries }: { entries: PlanEntry[] }) {
  const t = useT();
  return (
    <ol className="flex flex-col gap-1 text-chat">
      {entries.map((entry, n) => {
        const state = status(entry);
        return (
          <li key={n} data-status={state} className="flex items-start gap-2">
            <span
              role="img"
              aria-label={t(`stream:plan_${state}`)}
              className="mt-[3px] shrink-0 text-icon-tertiary"
            >
              <StepIcon status={state} />
            </span>
            <span className="shrink-0 text-fg-tertiary tabular-nums">{n + 1}.</span>
            <span className={state === "completed" ? "text-fg-secondary" : ""}>
              {escapeInvisible(entry.content)}
            </span>
          </li>
        );
      })}
    </ol>
  );
}

/** "计划", or "编写计划" while it grows; it opens on the plan's Markdown. */
function MarkdownPlan({ content, running }: { content: Content; running: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const blob = content.type === "blob" && "blob" in content ? (content.blob as BlobRef) : undefined;
  const full = useBlobText(blob);
  const inline = content.type === "text" && "text" in content ? content.text : undefined;
  const preview = blob && "preview" in content ? content.preview : undefined;
  const toggle = () => {
    if (!open && full.status === "idle") full.load();
    setOpen(!open);
  };
  return (
    <div className="rounded-lg border border-border">
      <button
        type="button"
        aria-expanded={open}
        onClick={toggle}
        className="flex w-full items-center gap-2 px-3 py-2 text-sm text-fg-secondary hover:text-fg"
      >
        <span className={`flex-1 text-left ${running ? "shimmer" : ""}`}>
          {t(running ? "stream:plan_writing" : "stream:plan")}
        </span>
        <span className={open ? "rotate-180" : ""}>
          <ChevronDownIcon />
        </span>
      </button>
      {open && (
        <div className="border-t border-border px-3 py-2">
          <Markdown text={String(inline ?? full.text ?? preview ?? "")} streaming={running} />
        </div>
      )}
    </div>
  );
}

function PlanRow({ row }: { row: RowOf<"plan"> }) {
  const { entries, markdown } = row.plan;
  if (entries?.length) return <Steps entries={entries} />;
  return markdown ? <MarkdownPlan content={markdown} running={row.running} /> : null;
}

registerRow("plan", PlanRow);
