import type { ComponentProps } from "react";

import { Tooltip } from "./Tooltip";

interface ToolbarButtonProps extends ComponentProps<"button"> {
  /** The accessible name, shown in the tooltip too. */
  label: string;
  shortcut?: string;
  /** A toggle's state; a menu trigger shows its open state by itself. */
  pressed?: boolean;
}

/**
 * The title bar's button (ui-spec 2.1): 28 high, tertiary, the hover colour
 * on hover, the secondary background while pressed or while its menu is open.
 */
export function ToolbarButton({
  label,
  shortcut,
  pressed,
  children,
  ...props
}: ToolbarButtonProps) {
  return (
    <Tooltip content={label} shortcut={shortcut}>
      <button
        type="button"
        aria-label={label}
        aria-pressed={pressed}
        {...props}
        className="app-no-drag flex h-7 items-center gap-1 rounded-lg px-2 text-sm text-fg-tertiary outline-none hover:bg-hover focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 aria-pressed:bg-btn-secondary data-[state=open]:bg-btn-secondary"
      >
        {children}
      </button>
    </Tooltip>
  );
}
