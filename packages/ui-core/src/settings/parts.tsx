// The settings page's building blocks (ui-spec 4.7): a section of rows, and a
// row with its label and description on the left, its control on the right.

import { useEffect, useRef, type ReactNode } from "react";

import type { Translate } from "../app/providers";

/** A section under its heading; `focused` brings it into view and gives it focus. */
export function Section(props: {
  title: string;
  action?: ReactNode;
  focused?: boolean;
  children: ReactNode;
}) {
  const ref = useRef<HTMLElement>(null);
  useEffect(() => {
    if (props.focused) ref.current?.focus();
  }, [props.focused]);
  return (
    <section
      ref={ref}
      aria-label={props.title}
      tabIndex={props.focused ? -1 : undefined}
      className="flex flex-col gap-2 outline-none"
    >
      <div className="flex h-7 items-center justify-between gap-3">
        <h2 className="text-base font-medium">{props.title}</h2>
        {props.action}
      </div>
      <div className="flex flex-col divide-y divide-border rounded-xl border border-border">
        {props.children}
      </div>
    </section>
  );
}

/** 12 × 16 padding, 24 between the text and the control; `value` is a plain text control. */
export function Row(props: {
  label: string;
  description?: string;
  value?: string;
  children?: ReactNode;
}) {
  return (
    <div className="flex items-center gap-6 px-4 py-3 text-sm">
      <div className="min-w-0 flex-1">
        <div className="font-medium">{props.label}</div>
        {props.description && (
          <div className="break-words text-fg-secondary">{props.description}</div>
        )}
      </div>
      {props.value && <span className="shrink-0 text-fg-secondary">{props.value}</span>}
      {props.children}
    </div>
  );
}

/** The text of an open enum's value under `key`; a value this build does not know reads "未知". */
export const known = (t: Translate, key: string) => t(key, { defaultValue: t("settings:unknown") });
