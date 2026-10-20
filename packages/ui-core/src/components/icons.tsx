// Drawn for cupper (F2: no icons from Codex); 16 × 16, in the text colour.

const svg = { width: 16, height: 16, viewBox: "0 0 16 16", fill: "none", "aria-hidden": true };

/** A pane outline with a divider: the sidebar (left) or the side panel (right). */
export function PaneIcon({ side }: { side: "left" | "right" }) {
  return (
    <svg {...svg} stroke="currentColor">
      <rect x="1.5" y="2.5" width="13" height="11" rx="2.5" />
      <path d={side === "left" ? "M6 2.5v11" : "M10 2.5v11"} />
    </svg>
  );
}

export function MoreIcon() {
  return (
    <svg {...svg} fill="currentColor">
      <circle cx="3.5" cy="8" r="1.25" />
      <circle cx="8" cy="8" r="1.25" />
      <circle cx="12.5" cy="8" r="1.25" />
    </svg>
  );
}

export function ChevronDownIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5 6.5l3 3 3-3" />
    </svg>
  );
}

export function FolderIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinejoin="round">
      <path d="M1.5 4.5a1 1 0 0 1 1-1h3.6l1.5 1.5h5.9a1 1 0 0 1 1 1v6.5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1z" />
    </svg>
  );
}

/** A pen on a page: a new session. */
export function ComposeIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d="M7 2.5H3.5a1 1 0 0 0-1 1v9a1 1 0 0 0 1 1h9a1 1 0 0 0 1-1V9" />
      <path d="M12 2l2 2-6 6H6V8z" />
    </svg>
  );
}

export function PlusIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round">
      <path d="M8 3v10M3 8h10" />
    </svg>
  );
}

export function ErrorIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round">
      <circle cx="8" cy="8" r="6" />
      <path d="M8 5v3.5M8 11v.01" />
    </svg>
  );
}

export function CloseIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round">
      <path d="M4.5 4.5l7 7M11.5 4.5l-7 7" />
    </svg>
  );
}

/** Turns in the text colour; still under reduced motion. */
export function Spinner({ className = "" }: { className?: string }) {
  return (
    <svg
      {...svg}
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      className={`animate-spin ${className}`}
    >
      <path d="M8 2a6 6 0 1 1-6 6" />
    </svg>
  );
}
