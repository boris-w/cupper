// The Toaster (design 5.7.4): top centre, 40 from the top, at most 790 wide
// and three at a time. Connection state, "handled elsewhere", "queued" and
// the outcome of privileged actions go here.

import {
  createContext,
  use,
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";

import { required, useT } from "../app/providers";
import { CloseIcon } from "../components/icons";

export interface Toast {
  /** A toast with the same id replaces the earlier one. */
  id: string;
  text: string;
  /** Stays until dismissed or replaced, instead of going after a few seconds. */
  sticky?: boolean;
}

export interface Toasts {
  show(toast: Toast): void;
  dismiss(id: string): void;
}

/** A new toast beyond this many pushes out the oldest. */
export const MAX_TOASTS = 3;
const LIFETIME_MS = 4000;

const ToastContext = createContext<Toasts | null>(null);

export function ToasterProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const dismiss = useCallback((id: string) => {
    setToasts((list) => list.filter((toast) => toast.id !== id));
  }, []);
  const show = useCallback((toast: Toast) => {
    setToasts((list) => [...list.filter(({ id }) => id !== toast.id), toast].slice(-MAX_TOASTS));
  }, []);
  return (
    <ToastContext value={useMemo(() => ({ show, dismiss }), [show, dismiss])}>
      {children}
      <ol className="pointer-events-none fixed inset-x-0 top-(--layout-toast-top) z-40 mx-auto flex max-w-(--layout-composer-adjacent-max-w) flex-col items-center gap-2 px-4">
        {toasts.map((toast) => (
          <ToastView key={toast.id} toast={toast} dismiss={dismiss} />
        ))}
      </ol>
    </ToastContext>
  );
}

export const useToasts = () => required(use(ToastContext), "ToasterProvider");

function ToastView({ toast, dismiss }: { toast: Toast; dismiss: (id: string) => void }) {
  const t = useT();
  useEffect(() => {
    if (toast.sticky) {
      return;
    }
    const timer = setTimeout(() => dismiss(toast.id), LIFETIME_MS);
    return () => clearTimeout(timer);
  }, [toast, dismiss]);
  return (
    <li
      role="status"
      className="pointer-events-auto flex items-center gap-2 rounded-xl bg-surface-elevated py-1.5 pr-1.5 pl-3 text-sm shadow-menu"
    >
      {toast.text}
      <button
        type="button"
        aria-label={t("shell:dismiss")}
        onClick={() => dismiss(toast.id)}
        className="rounded-md p-0.5 text-fg-tertiary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring"
      >
        <CloseIcon />
      </button>
    </li>
  );
}
