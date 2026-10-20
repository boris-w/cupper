import { useRef, useState } from "react";

import { dragResize, keyResize, type Bounds } from "../layout/layout";

interface SplitterProps {
  label: string;
  /** The panel's edge the handle sits on; the panel widens away from it. */
  edge: "left" | "right";
  width: number;
  bounds: Bounds;
  /** Every move of a drag with `done: false`, then its end, or one key press, with `done: true`. */
  onResize(width: number, done: boolean): void;
}

/**
 * A resize handle (design 5.7.4): `role=separator`, arrow keys by a step, Home
 * the minimum, End the maximum. Its 1px line shows on hover, drag and focus.
 */
export function Splitter({ label, edge, width, bounds, onResize }: SplitterProps) {
  const drag = useRef<{ start: number; x: number; width: number } | null>(null);
  const [dragging, setDragging] = useState(false);
  const end = () => {
    if (drag.current) {
      onResize(drag.current.width, true);
      drag.current = null;
      setDragging(false);
    }
  };
  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={label}
      aria-valuemin={bounds.min}
      aria-valuemax={bounds.max}
      aria-valuenow={width}
      tabIndex={0}
      data-dragging={dragging || undefined}
      className={`group app-no-drag absolute inset-y-0 z-10 w-2 cursor-col-resize outline-none ${edge === "right" ? "-right-1" : "-left-1"}`}
      onPointerDown={(event) => {
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { start: width, x: event.clientX, width };
        setDragging(true);
      }}
      onPointerMove={(event) => {
        if (drag.current) {
          const { start, x } = drag.current;
          drag.current.width = dragResize(start, x, event.clientX, bounds, edge);
          onResize(drag.current.width, false);
        }
      }}
      onPointerUp={end}
      onLostPointerCapture={end}
      onKeyDown={(event) => {
        const next = keyResize(
          event.key,
          width,
          bounds,
          edge === "right" ? "ArrowRight" : "ArrowLeft",
        );
        if (next !== undefined) {
          event.preventDefault();
          onResize(next, true);
        }
      }}
    >
      <div className="mx-auto h-full w-px group-hover:bg-border-strong group-focus-visible:bg-border-strong group-data-dragging:bg-border-strong" />
    </div>
  );
}
