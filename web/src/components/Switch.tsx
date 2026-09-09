// An accessible toggle (Radix), styled as the design's green switches.

import * as RadixSwitch from "@radix-ui/react-switch";

export function Switch({
  checked,
  onChange,
  disabled,
  label,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  label: string;
}) {
  return (
    <RadixSwitch.Root
      checked={checked}
      onCheckedChange={onChange}
      disabled={disabled}
      aria-label={label}
      className="relative h-[22px] w-10 shrink-0 rounded-full bg-line transition-colors data-[state=checked]:bg-accent disabled:opacity-50"
    >
      <RadixSwitch.Thumb className="block h-[18px] w-[18px] translate-x-0.5 rounded-full bg-white shadow transition-transform data-[state=checked]:translate-x-[20px]" />
    </RadixSwitch.Root>
  );
}
