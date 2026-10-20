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
