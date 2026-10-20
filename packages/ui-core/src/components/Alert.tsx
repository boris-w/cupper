import type { ReactNode } from "react";

/**
 * The warning box (ui-spec 4.3) of banners, notices and errors: a 1px border
 * in the tone at 30%, 12 × 8 padding, at most 77% of the session column.
 * `action` is a button inside, such as a recovery step.
 */
export function Alert(props: {
  tone?: "warning" | "danger";
  children: ReactNode;
  action?: ReactNode;
}) {
  const danger = props.tone === "danger";
  return (
    <div
      role={danger ? "alert" : "status"}
      className={`flex max-w-[77%] items-center gap-3 rounded-lg border px-3 py-2 text-sm ${danger ? "border-danger/30" : "border-warning/30"}`}
    >
      <div className="min-w-0 flex-1">{props.children}</div>
      {props.action}
    </div>
  );
}
