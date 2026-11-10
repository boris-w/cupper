// The composer's popover lists, `@` files and `/` commands (design 5.7.4):
// they share the popover container and the look of T142's list. While one
// is open its keys come before the composer's: ↑↓ pick, Enter or Tab takes
// the picked row, Esc closes. An input method's keys stay its own (CP-1).

import { use, useEffect, useRef, useState } from "react";

import { OverlayContext } from "./extensions";
import { composing } from "./keys";

/** The list's frame, above the composer. */
export const POPOVER =
  "absolute inset-x-0 bottom-2 z-20 max-h-72 overflow-y-auto rounded-2xl bg-surface-elevated p-1 text-sm shadow-card";
/** A row of the list; `aria-selected` marks the picked one. */
export const OPTION = "truncate rounded-lg px-2.5 py-1.5 text-fg-secondary aria-selected:bg-hover";

/** Where the caret is in the text area, as typing, clicks and keys move it. */
export function useCaret() {
  const { area } = use(OverlayContext);
  const [caret, setCaret] = useState(0);
  useEffect(() => {
    if (!area) return;
    const follow = () => setCaret(area.selectionStart);
    const events = ["input", "keyup", "click", "focus"] as const;
    events.forEach((name) => area.addEventListener(name, follow));
    return () => events.forEach((name) => area.removeEventListener(name, follow));
  }, [area]);
  return [caret, setCaret] as const;
}

/** An open list of `count` rows: the picked one, and what picking, taking and closing do. */
export interface PopoverList {
  count: number;
  selected: number;
  select: (index: number) => void;
  take: (index: number) => void;
  close: () => void;
}

/** While `open`, the text area's keys go to `list` first, and Esc closes it. */
export function usePopoverKeys(open: boolean, list: PopoverList) {
  const { area, setClose } = use(OverlayContext);
  const latest = useRef(list);
  latest.current = list;
  useEffect(() => {
    if (!area || !open) return;
    setClose(() => latest.current.close());
    const onKey = (event: KeyboardEvent) => {
      if (composing(event)) return;
      const { count, selected, select, take } = latest.current;
      const move = { ArrowDown: 1, ArrowUp: -1 }[event.key];
      const pick = event.key === "Enter" || event.key === "Tab";
      if ((move === undefined && !pick) || count === 0) return;
      event.preventDefault();
      event.stopPropagation();
      if (move !== undefined) {
        select((selected + move + count) % count);
      } else {
        take(selected);
      }
    };
    area.addEventListener("keydown", onKey);
    return () => {
      area.removeEventListener("keydown", onKey);
      setClose(null);
    };
  }, [area, open, setClose]);
}
