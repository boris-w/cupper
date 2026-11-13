import * as RadixContextMenu from "@radix-ui/react-context-menu";
import * as RadixMenu from "@radix-ui/react-dropdown-menu";
import { useRef, type ReactElement, type ReactNode } from "react";

import { CheckIcon } from "./icons";

const CONTENT = "z-50 min-w-40 rounded-xl bg-surface-elevated p-1 text-sm shadow-menu";
const ITEM =
  "flex items-center rounded-xl px-2 py-1.25 outline-none data-disabled:text-fg-tertiary data-highlighted:bg-hover";

/** A menu on `trigger`, such as a ToolbarButton (ui-spec 3.4–3.6); `children` are its items. */
export function Menu(props: {
  trigger: ReactElement;
  children?: ReactNode;
  /** Where the menu lines up with its trigger; its end by default. */
  align?: "start" | "center" | "end";
  /** Opened and closed from outside, too. */
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** As the menu closes, before it gives the focus back; `preventDefault` keeps it where it is. */
  onCloseAutoFocus?: (event: Event) => void;
}) {
  // Closed by a key, the menu gives the focus back to its trigger; closed by
  // the pointer, it does not, or the trigger's tooltip and focus ring would
  // show after a click.
  const pointer = useRef(false);
  return (
    <RadixMenu.Root open={props.open} onOpenChange={props.onOpenChange}>
      <RadixMenu.Trigger asChild>{props.trigger}</RadixMenu.Trigger>
      <RadixMenu.Portal>
        <RadixMenu.Content
          onPointerDown={() => (pointer.current = true)}
          onPointerDownOutside={() => (pointer.current = true)}
          onKeyDown={() => (pointer.current = false)}
          onCloseAutoFocus={(event) => {
            if (pointer.current) event.preventDefault();
            pointer.current = false;
            props.onCloseAutoFocus?.(event);
          }}
          align={props.align ?? "end"}
          sideOffset={4}
          className={CONTENT}
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
  /** A choice: a check at its end when true, room for one when false. */
  checked?: boolean;
  /** One choice of several, rather than a switch on its own. */
  radio?: boolean;
  children: ReactNode;
}) {
  const choice = props.checked !== undefined;
  return (
    <RadixMenu.Item
      onSelect={props.onSelect}
      disabled={props.disabled}
      role={!choice ? "menuitem" : props.radio ? "menuitemradio" : "menuitemcheckbox"}
      aria-checked={props.checked}
      className={ITEM}
    >
      {props.children}
      {choice && (
        <span className="ms-auto flex w-8 shrink-0 justify-end">
          {props.checked && <CheckIcon />}
        </span>
      )}
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

/** A menu at the pointer on a right-click on `children`, with the look of `Menu`. */
export function ContextMenu(props: { children: ReactElement; items: ReactNode }) {
  return (
    <RadixContextMenu.Root>
      <RadixContextMenu.Trigger asChild>{props.children}</RadixContextMenu.Trigger>
      <RadixContextMenu.Portal>
        <RadixContextMenu.Content className={CONTENT}>{props.items}</RadixContextMenu.Content>
      </RadixContextMenu.Portal>
    </RadixContextMenu.Root>
  );
}

export const ContextMenuItem = (props: { onSelect(): void; children: ReactNode }) => (
  <RadixContextMenu.Item onSelect={props.onSelect} className={ITEM}>
    {props.children}
  </RadixContextMenu.Item>
);
