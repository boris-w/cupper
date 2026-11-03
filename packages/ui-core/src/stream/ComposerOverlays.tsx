// What floats above the composer (design 5.7.5 `<ComposerOverlays/>`): the
// plan pill "第 n / m 步" while a turn runs with a plan (F3), and the way back
// to the bottom. Both move the stream list, so the list draws them at its foot.

import { useT } from "../app/providers";
import { ArrowUpIcon } from "../components/icons";
import type { RowOf, StreamRow } from "../rows/stream";

export interface PlanPill {
  /** The plan's row. */
  key: string;
  /** The step under way, from 1: the one in progress, else the first not done. */
  n: number;
  m: number;
}

/** The steps of the plan in the running turn, if there is one. */
export function runningPlan(rows: StreamRow[]): PlanPill | undefined {
  let running = false;
  let plan: RowOf<"plan"> | undefined;
  for (const row of rows) {
    if (row.kind === "turn_header") running = row.status === "running";
    else if (row.kind === "plan") plan = running && row.plan.entries?.length ? row : undefined;
  }
  const entries = plan?.plan.entries ?? [];
  if (!plan) return undefined;
  const at = entries.findIndex((entry) => entry.status === "in_progress");
  const next = entries.findIndex((entry) => entry.status !== "completed");
  const n = (at >= 0 ? at : next >= 0 ? next : entries.length - 1) + 1;
  return { key: plan.key, n, m: entries.length };
}

export function ComposerOverlays(props: {
  plan?: PlanPill;
  onPlan(): void;
  /** Scrolled up: the list no longer follows the stream. */
  away: boolean;
  onBottom(): void;
}) {
  const t = useT();
  const { plan } = props;
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-2 flex items-center justify-center gap-2">
      {plan && (
        <button
          type="button"
          onClick={props.onPlan}
          className="pointer-events-auto h-7 rounded-full bg-surface-elevated px-3 text-sm text-fg-secondary shadow-pill hover:text-fg"
        >
          {t("stream:plan_step", { n: plan.n, m: plan.m })}
        </button>
      )}
      {props.away && (
        <button
          type="button"
          aria-label={t("stream:to_bottom")}
          onClick={props.onBottom}
          className="pointer-events-auto flex size-8 rotate-180 items-center justify-center rounded-full border border-border bg-bg shadow-sm hover:bg-hover"
        >
          <ArrowUpIcon />
        </button>
      )}
    </div>
  );
}
