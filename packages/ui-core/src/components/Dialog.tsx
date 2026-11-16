// The modal dialog (design 5.7.4): the native <dialog>, shown modal while it
// is mounted, so that the page behind takes neither the focus nor the pointer
// and the focus goes back where it was once it closes. Esc or "取消" closes
// it, unless an action runs. Its body is a form: Enter in a field submits it.

import { useId, useLayoutEffect, useRef, type ReactNode } from "react";

import { useT } from "../app/providers";

const BUTTON =
  "h-8 rounded-full px-3.5 outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50";
export const SECONDARY = `${BUTTON} bg-btn-secondary hover:bg-hover`;
export const PRIMARY = `${BUTTON} bg-btn-primary text-btn-primary-fg`;
export const DANGER = `${BUTTON} bg-danger text-white hover:opacity-90`;

export function Dialog(props: {
  title: string;
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
  // Closed before React takes it out of the document, which gives the focus back.
  useLayoutEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      aria-labelledby={title}
      onCancel={(event) => {
        event.preventDefault();
        if (!props.busy) props.onClose();
      }}
      className="m-auto w-105 rounded-2xl bg-surface-elevated p-5 text-sm text-fg shadow-menu backdrop:bg-backdrop"
    >
      <form
        className="flex flex-col gap-3"
        onSubmit={(event) => {
          event.preventDefault();
          if (!props.busy) props.onSubmit?.();
        }}
      >
        <h2 id={title} className="text-base font-medium">
          {props.title}
        </h2>
        {props.children}
        <fieldset disabled={props.busy} className="flex justify-end gap-2">
          <button type="button" onClick={props.onClose} className={SECONDARY}>
            {t("session:cancel")}
          </button>
          {props.actions}
        </fieldset>
      </form>
    </dialog>
  );
}
