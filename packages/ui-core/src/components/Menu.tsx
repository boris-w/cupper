import * as RadixMenu from "@radix-ui/react-dropdown-menu";
import type { ReactElement, ReactNode } from "react";

/** A menu on `trigger`, such as a ToolbarButton (ui-spec 3.4–3.6); `children` are its items. */
export function Menu(props: {
  trigger: ReactElement;
  children?: ReactNode;
  /** Where the menu lines up with its trigger; its end by default. */
  align?: "start" | "center" | "end";
}) {
  return (
    <RadixMenu.Root>
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
  children: ReactNode;
}) {
  return (
    <RadixMenu.Item
      onSelect={props.onSelect}
      disabled={props.disabled}
      className="flex items-center rounded-xl px-2 py-1.25 outline-none data-disabled:text-fg-tertiary data-highlighted:bg-hover"
    >
      {props.children}
    </RadixMenu.Item>
  );
}

export const MenuSeparator = () => <RadixMenu.Separator className="mx-2 my-1 h-px bg-border" />;
