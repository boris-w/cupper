import * as RadixSwitch from "@radix-ui/react-switch";

/** A switch (ui-spec 4.7): 32 × 19, a 13 thumb, the accent colour when on. */
export function Switch(props: {
  label: string;
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <RadixSwitch.Root
      aria-label={props.label}
      checked={props.checked}
      onCheckedChange={props.onCheckedChange}
      disabled={props.disabled}
      className="inline-flex h-4.75 w-8 shrink-0 items-center rounded-full bg-active px-0.75 outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 data-[state=checked]:bg-accent"
    >
      <RadixSwitch.Thumb className="size-3.25 rounded-full bg-surface-elevated transition-transform duration-(--duration-fast) data-[state=checked]:translate-x-3.25" />
    </RadixSwitch.Root>
  );
}
