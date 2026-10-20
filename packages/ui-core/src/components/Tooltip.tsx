import * as RadixTooltip from "@radix-ui/react-tooltip";
import type { ReactElement, ReactNode } from "react";

/** Around the app once: tooltips share the delay (ui-spec 2.1). */
export function TooltipProvider({ children }: { children: ReactNode }) {
  return <RadixTooltip.Provider delayDuration={500}>{children}</RadixTooltip.Provider>;
}

/** A tooltip on `children`, which takes the trigger's props and ref; `shortcut` follows the text. */
export function Tooltip(props: { content: ReactNode; shortcut?: string; children: ReactElement }) {
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{props.children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          sideOffset={4}
          className="z-50 flex items-center gap-2 rounded-2xl bg-tooltip px-2.5 py-1 text-sm text-tooltip-fg"
        >
          {props.content}
          {props.shortcut && <kbd className="font-sans opacity-60">{props.shortcut}</kbd>}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}
