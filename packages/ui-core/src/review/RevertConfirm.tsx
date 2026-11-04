// The revert confirmation (design 5.7.5, 5.7.8 RP-11): reverting loses the
// user's changes and cannot be undone, so nothing reverts without a click or a
// key on this control, in the main document, where the main process records
// real input. It is armed after 500 ms: keys and clicks pressed before do
// nothing. A revert from the "还原全部" button puts the focus on "确认"; one
// from the review document takes no focus, as the user may be typing a
// comment there. There is no "don't ask again".

import { useEffect, useId, useRef, useState, type MouseEvent } from "react";

import { useT } from "../app/providers";
import { ARMING_MS } from "../request/RequestPanel";
import type { PendingRevert } from "./actions";

const BUTTON =
  "h-7 rounded-full px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring";

export function RevertConfirm(props: { pending: PendingRevert; settle(confirmed: boolean): void }) {
  const t = useT();
  const title = useId();
  const confirm = useRef<HTMLButtonElement>(null);
  const [ready, setReady] = useState(false);
  // A click counts only when its button went down once the control was armed.
  const downWhileReady = useRef(false);
  const { source, path, hunk } = props.pending;

  useEffect(() => {
    if (source === "button") {
      confirm.current?.focus();
    }
    const timer = setTimeout(() => setReady(true), ARMING_MS);
    return () => clearTimeout(timer);
  }, [source]);

  // A keyboard click has no pointer: `detail` is 0.
  const armed = (event: MouseEvent) => ready && (event.detail === 0 || downWhileReady.current);
  const pressed = () => void (downWhileReady.current = ready);
  const heading = hunk
    ? t("review:revert_title_hunk")
    : path
      ? t("review:revert_title_path", { path })
      : t("review:revert_title");

  return (
    <div
      role="alertdialog"
      aria-labelledby={title}
      data-ready={ready || undefined}
      onKeyDown={(event) => event.key === "Escape" && ready && props.settle(false)}
      className="absolute inset-x-4 bottom-18 z-10 mx-auto max-w-90 rounded-2xl bg-surface-elevated p-4 shadow-pill"
    >
      <h2 id={title} className="truncate text-sm font-medium">
        {heading}
      </h2>
      <p className="mt-1 text-sm text-fg-secondary">{t("review:revert_body")}</p>
      <div className="mt-3 flex justify-end gap-2">
        <button
          type="button"
          onPointerDown={pressed}
          onClick={(event) => armed(event) && props.settle(false)}
          className={`${BUTTON} bg-btn-secondary hover:bg-hover`}
        >
          {t("review:cancel")}
        </button>
        <button
          ref={confirm}
          type="button"
          onPointerDown={pressed}
          onClick={(event) => armed(event) && props.settle(true)}
          className={`${BUTTON} bg-danger text-white hover:opacity-90`}
        >
          {t("review:confirm")}
        </button>
      </div>
    </div>
  );
}
