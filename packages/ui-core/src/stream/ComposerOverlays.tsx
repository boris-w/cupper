// What floats above the composer (design 5.7.5 `<ComposerOverlays/>`): the
// plan pill "第 n / m 步" while a turn runs with a plan (F3), and the way back
// to the bottom. Both move the stream list, so the list draws them at its foot,
// 24 above the composer. The way back is a 32 circle; while a turn runs it is
// 40 wide with three dots, its arrow back on hover, as Codex's (design 5.7.4).

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
  /** Scrolled up, more than 112 from the end: the list no longer follows the stream. */
  away: boolean;
  /** A turn of the session runs. */
  running: boolean;
  onBottom(): void;
}) {
  const t = useT();
  const { plan } = props;
  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-6 flex items-center justify-center gap-2">
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
          data-running={props.running ? "" : undefined}
          className="group pointer-events-auto flex h-8 w-8 items-center justify-center rounded-full border border-border bg-surface text-fg shadow-card transition-[width] duration-fast hover:bg-hover data-running:w-10 data-running:hover:w-8"
        >
          {props.running && (
            <span className="flex gap-1 group-hover:hidden" data-dots>
              {["", "[animation-delay:150ms]", "[animation-delay:300ms]"].map((delay) => (
                <span key={delay} className={`size-1 animate-dot rounded-full bg-fg/70 ${delay}`} />
              ))}
            </span>
          )}
          <span className="rotate-180 group-data-running:hidden group-data-running:group-hover:block">
            <ArrowUpIcon />
          </span>
        </button>
      )}
    </div>
  );
}
