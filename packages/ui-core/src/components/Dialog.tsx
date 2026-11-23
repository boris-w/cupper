// The modal dialog (design 5.7.4): the native <dialog>, shown modal while it
// is mounted, so that the page behind takes neither the focus nor the pointer
// and the focus goes back where it was once it closes. Esc or "取消" closes
// it, unless an action runs. Its body is a form: Enter in a field submits it.
// The focus goes back to where it was before, as a field it takes may have
// it first; when that was a menu's item, gone now, closed by a key the dialog
// gives it to the menu's trigger, as the menus do (T276, I5).

import { useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import { useT } from "../app/providers";
import { lastMenuTrigger } from "./Menu";

const BUTTON =
  "h-8 rounded-full px-3.5 outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
export const SECONDARY = `${BUTTON} bg-btn-secondary hover:bg-hover`;
export const PRIMARY = `${BUTTON} bg-btn-primary text-btn-primary-fg`;
export const DANGER = `${BUTTON} bg-danger text-white hover:opacity-90`;

export function Dialog(props: {
  title: string;
  /** A line under the title. */
  description?: ReactNode;
  /** A word at the buttons' start, such as what continuing runs. */
  note?: ReactNode;
  /** An action runs: the dialog stays, its buttons disabled. */
  busy?: boolean;
  onClose(): void;
  onSubmit?(): void;
  /** The buttons after "取消", the default one last. */
  actions: ReactNode;
  children?: ReactNode;
}) {
  const t = useT();
  const title = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const pointer = useRef(false);
  // Read as it renders, before an autofocused field moves the focus.
  const [before] = useState(() => document.activeElement);
  // Closed before React takes it out of the document.
  useLayoutEffect(() => {
    const element = dialog.current;
    const trigger = lastMenuTrigger();
    element?.showModal();
    return () => {
      element?.close();
      const focused = document.activeElement;
      if (focused && focused !== document.body && !element?.contains(focused)) return;
      const kept = before instanceof HTMLElement && before.isConnected && before !== document.body;
      (kept ? before : pointer.current ? null : trigger)?.focus();
    };
  }, [before]);
  return (
    <dialog
      ref={dialog}
      aria-labelledby={title}
      onPointerDown={() => (pointer.current = true)}
      onKeyDown={() => (pointer.current = false)}
      onCancel={(event) => {
        event.preventDefault();
        if (!props.busy) props.onClose();
      }}
      className="m-auto w-105 rounded-xl bg-surface-elevated p-5 text-sm text-fg shadow-menu backdrop:bg-backdrop"
    >
      <form
        className="flex flex-col gap-3 [&_[role]]:max-w-none"
        onSubmit={(event) => {
          event.preventDefault();
          if (!props.busy) props.onSubmit?.();
        }}
      >
        <div>
          <h2 id={title} className="text-base font-medium">
            {props.title}
          </h2>
          {props.description && <p className="text-fg-secondary">{props.description}</p>}
        </div>
        {props.children}
        <fieldset disabled={props.busy} className="flex items-center justify-end gap-2">
          {props.note && <span className="mr-auto text-fg-tertiary">{props.note}</span>}
          <button type="button" onClick={props.onClose} className={SECONDARY}>
            {t("session:cancel")}
          </button>
          {props.actions}
        </fieldset>
      </form>
    </dialog>
  );
}
