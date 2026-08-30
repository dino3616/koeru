import * as SelectPrimitive from "@radix-ui/react-select";
import type { ComponentProps } from "react";

import { cn } from "~/lib/cn";

export const Select = SelectPrimitive.Root;
export const SelectValue = SelectPrimitive.Value;

export const SelectTrigger = ({
  className,
  children,
  ...props
}: ComponentProps<typeof SelectPrimitive.Trigger>) => (
  <SelectPrimitive.Trigger
    className={cn(
      "flex h-11 w-full items-center justify-between gap-2 rounded-lg",
      "border border-border-strong bg-surface-2 px-3 text-sm text-text",
      "disabled:opacity-45 data-[placeholder]:text-text-dim",
      className,
    )}
    {...props}
  >
    {children}
    <SelectPrimitive.Icon aria-hidden="true">▾</SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
);

export const SelectContent = ({
  className,
  children,
  ...props
}: ComponentProps<typeof SelectPrimitive.Content>) => (
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      position="popper"
      sideOffset={4}
      className={cn(
        "z-50 min-w-[var(--radix-select-trigger-width)] overflow-hidden",
        "rounded-lg border border-border bg-surface p-1 shadow-xl",
        className,
      )}
      {...props}
    >
      <SelectPrimitive.Viewport>{children}</SelectPrimitive.Viewport>
    </SelectPrimitive.Content>
  </SelectPrimitive.Portal>
);

export const SelectItem = ({
  className,
  children,
  ...props
}: ComponentProps<typeof SelectPrimitive.Item>) => (
  <SelectPrimitive.Item
    className={cn(
      "flex h-10 cursor-default select-none items-center rounded-md px-3 text-sm",
      "data-[highlighted]:bg-surface-2 data-[highlighted]:outline-none",
      className,
    )}
    {...props}
  >
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
  </SelectPrimitive.Item>
);
