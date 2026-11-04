import * as RadixMenu from "@radix-ui/react-dropdown-menu";
import type { ReactElement, ReactNode } from "react";

import { CheckIcon } from "./icons";

/** A menu on `trigger`, such as a ToolbarButton (ui-spec 3.4–3.6); `children` are its items. */
export function Menu(props: {
  trigger: ReactElement;
  children?: ReactNode;
  /** Where the menu lines up with its trigger; its end by default. */
  align?: "start" | "center" | "end";
  /** Opened and closed from outside, too. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
  return (
    <RadixMenu.Root open={props.open} onOpenChange={props.onOpenChange}>
      <RadixMenu.Trigger asChild>{props.trigger}</RadixMenu.Trigger>
      <RadixMenu.Portal>
        <RadixMenu.Content
          align={props.align ?? "end"}
          sideOffset={4}
          className="z-50 min-w-40 rounded-xl bg-surface-elevated p-1 text-sm shadow-menu"
        >
          {props.children}
        </RadixMenu.Content>
      </RadixMenu.Portal>
    </RadixMenu.Root>
  );
}

export function MenuItem(props: {
  onSelect?: () => void;
  disabled?: boolean;
  /** A choice: a check before it when true, room for one when false. */
  checked?: boolean;
  /** One choice of several, rather than a switch on its own. */
  radio?: boolean;
  children: ReactNode;
}) {
  return (
    <RadixMenu.Item
      onSelect={props.onSelect}
      disabled={props.disabled}
      role={
        props.checked === undefined
          ? "menuitem"
          : props.radio
            ? "menuitemradio"
            : "menuitemcheckbox"
      }
      aria-checked={props.checked}
      className="flex items-center gap-1.5 rounded-xl px-2 py-1.25 outline-none data-disabled:text-fg-tertiary data-highlighted:bg-hover"
    >
      {props.checked !== undefined && (
        <span className="w-4 shrink-0">{props.checked && <CheckIcon />}</span>
      )}
      {props.children}
    </RadixMenu.Item>
  );
}

export const MenuSeparator = () => <RadixMenu.Separator className="mx-2 my-1 h-px bg-border" />;

/** A menu's heading, such as "查阅来源". */
export function MenuLabel({ children }: { children: ReactNode }) {
  return (
    <RadixMenu.Label className="px-2 py-1 text-xs text-fg-tertiary">{children}</RadixMenu.Label>
  );
}
