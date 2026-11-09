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

/** A plan step's status (MS-4): an empty ring, a ring with a dot, or a ring with a tick. */
export function StepIcon({ status }: { status: "pending" | "in_progress" | "completed" }) {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="8" cy="8" r="6" />
      {status === "in_progress" && <circle cx="8" cy="8" r="2.5" fill="currentColor" />}
      {status === "completed" && <path d="M5.5 8.2l1.7 1.7 3.3-3.6" />}
    </svg>
  );
}

/** A shield: the permission mode. */
export function ShieldIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinejoin="round">
      <path d="M8 1.5l5.5 2v4c0 3.2-2.3 5.8-5.5 7-3.2-1.2-5.5-3.8-5.5-7v-4z" />
    </svg>
  );
}

export function CheckIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3.5 8.5l3 3 6-7" />
    </svg>
  );
}

export function MinusIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round">
      <path d="M3 8h10" />
    </svg>
  );
}

/** A gear: the settings. */
export function SettingsIcon() {
  return (
    <svg {...svg} stroke="currentColor">
      <circle cx="8" cy="8" r="5.5" strokeWidth="2.5" strokeDasharray="1.75 2.57" />
      <circle cx="8" cy="8" r="4.25" />
      <circle cx="8" cy="8" r="1.75" />
    </svg>
  );
}

/** An arrow to the left: back. */
export function BackIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d="M13 8H3M7 4L3 8l4 4" />
    </svg>
  );
}

/** An arrow turning back: revert. */
export function UndoIcon() {
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round" strokeLinejoin="round">
      <path d="M5.5 3.5L2.5 6.5l3 3" />
      <path d="M2.5 6.5h7a3.5 3.5 0 0 1 0 7h-3" />
    </svg>
  );
}

/** A file's git status in a rounded square: added (+), deleted (−), changed (•). */
export function GitStatusIcon({ change }: { change: string }) {
  const mark = { add: "M8 5v6M5 8h6", delete: "M5 8h6" }[change];
  return (
    <svg {...svg} stroke="currentColor" strokeLinecap="round">
      <rect x="2" y="2" width="12" height="12" rx="3" />
      {mark ? <path d={mark} /> : <circle cx="8" cy="8" r="1.5" fill="currentColor" />}
    </svg>
  );
}
