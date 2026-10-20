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

export function ArrowUpIcon() {
  return (
    <svg
      {...svg}
      stroke="currentColor"
      strokeWidth="1.75"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M8 13V3.5M4 7.5l4-4 4 4" />
    </svg>
  );
}

export function StopIcon() {
  return (
    <svg {...svg} fill="currentColor">
      <rect x="4.5" y="4.5" width="7" height="7" rx="1.5" />
    </svg>
  );
}

/** The tool kinds' icons (design 5.7.4 MS-3). */
export function ToolIcon({ kind }: { kind: string }) {
  const paths: Record<string, string> = {
    read: "M4 2.5h5.5L12 5v8.5H4zM9.5 2.5V5H12",
    edit: "M10.5 3l2.5 2.5L6 12.5H3.5V10z",
    delete: "M3 4.5h10M6 4.5v-2h4v2M4.5 4.5l.5 9h6l.5-9",
    move: "M2.5 8h10M9.5 5l3 3-3 3",
    search: "M7 11.5a4.5 4.5 0 1 0 0-9 4.5 4.5 0 0 0 0 9zM10.5 10.5l3 3",
    execute: "M2.5 3.5h11v9h-11zM5 6.5l2 1.5-2 1.5M8.5 10H11",
    fetch:
      "M8 13.5a5.5 5.5 0 1 0 0-11 5.5 5.5 0 0 0 0 11zM2.5 8h11M8 2.5c-2 2.5-2 8.5 0 11M8 2.5c2 2.5 2 8.5 0 11",
  };
  const d = paths[kind] ?? "M8 9a1 1 0 1 0 0-2 1 1 0 0 0 0 2z";
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d={d} />
    </svg>
  );
}
