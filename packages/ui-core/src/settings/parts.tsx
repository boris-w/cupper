// The settings page's building blocks (ui-spec 4.7): a section of rows, and a
// row with its label and description on the left, its control on the right.

import type { SettingsPatch } from "@cupper/protocol";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { useHost, useT, type Translate } from "../app/providers";
import { Switch } from "../components/Switch";
import { codeOf } from "../composer/send";
import { useToasts } from "../feedback/toaster";
import { useStreams } from "../state/provider";
import { useEntry } from "../state/stream";

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

/** A row with a switch, off and disabled until `checked` is known. */
export function SwitchRow(props: {
  label: string;
  description?: string;
  checked: boolean | undefined;
  onChange: (checked: boolean) => void;
}) {
  return (
    <Row label={props.label} description={props.description}>
      <Switch
        label={props.label}
        checked={props.checked ?? false}
        disabled={props.checked === undefined}
        onCheckedChange={props.onChange}
      />
    </Row>
  );
}

/**
 * The host's settings as the index stream has them, once it has; `update`
 * changes some of them with `settings/update`, and a toast says when it fails.
 */
export function useSettings() {
  const t = useT();
  const { client } = useHost();
  const { show } = useToasts();
  const { view } = useStreams().index;
  useEntry(view, "settings");
  const update = (patch: SettingsPatch) =>
    void client.command("settings/update", { patch }).catch((error: unknown) => {
      show({ id: "settings", text: t("settings:update_failed", { code: codeOf(error) }) });
    });
  return { settings: view.toJSON().settings?.settings, update };
}

export const BUTTON =
  "shrink-0 rounded-lg bg-btn-secondary px-2.5 py-1 text-sm outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";

/**
 * A command's button, `settings:<name>`: `<name>_busy` until the host
 * answers; a toast says `<name>_failed`, or the text `run` resolves to.
 */
export function CommandButton({ name, run }: { name: string; run: () => Promise<unknown> }) {
  const t = useT();
  const { show } = useToasts();
  const [busy, setBusy] = useState(false);
  const click = () => {
    setBusy(true);
    run()
      .then((text) => typeof text === "string" && show({ id: name, text }))
      .catch((error: unknown) =>
        show({ id: name, text: t(`settings:${name}_failed`, { code: codeOf(error) }) }),
      )
      .finally(() => setBusy(false));
  };
  return (
    <button type="button" disabled={busy} onClick={click} className={BUTTON}>
      {t(`settings:${name}${busy ? "_busy" : ""}`)}
    </button>
  );
}

/** The text of an open enum's value under `key`; a value this build does not know reads "未知". */
export const known = (t: Translate, key: string) => t(key, { defaultValue: t("settings:unknown") });
