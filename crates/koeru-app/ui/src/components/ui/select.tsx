import * as SelectPrimitive from "@radix-ui/react-select";
import type { ComponentProps } from "react";

/*
 * 一覧から1つ選ぶ。
 *
 * Radix Primitives のまま出す（`DEC-PLT-015`）。 フォーカス管理と
 * 型入力での移動、`aria-*` の付け外しはあちらが持っている。
 *
 * どれも `className` を受け取らない。 見た目は部品が持つ（`~/lib/tv` の冒頭）。
 */

export const Select = SelectPrimitive.Root;
export const SelectValue = SelectPrimitive.Value;

export const SelectTrigger = ({
  children,
  ...props
}: Omit<ComponentProps<typeof SelectPrimitive.Trigger>, "className">) => (
  <SelectPrimitive.Trigger
    className="flex h-11 w-full items-center justify-between gap-2 rounded-lg border border-slate-11 bg-slate-3 px-3 text-sm text-slate-12 disabled:opacity-45 data-[placeholder]:text-slate-11"
    {...props}
  >
    {children}
    <SelectPrimitive.Icon aria-hidden="true">▾</SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
);

export const SelectContent = ({
  children,
  ...props
}: Omit<ComponentProps<typeof SelectPrimitive.Content>, "className">) => (
  <SelectPrimitive.Portal>
    <SelectPrimitive.Content
      position="popper"
      sideOffset={4}
      className="z-50 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-lg border border-slate-6 bg-slate-2 p-1 shadow-xl"
      {...props}
    >
      <SelectPrimitive.Viewport>{children}</SelectPrimitive.Viewport>
    </SelectPrimitive.Content>
  </SelectPrimitive.Portal>
);

export const SelectItem = ({
  children,
  ...props
}: Omit<ComponentProps<typeof SelectPrimitive.Item>, "className">) => (
  <SelectPrimitive.Item
    className="flex h-10 cursor-default select-none items-center rounded-md px-3 text-sm data-[highlighted]:bg-slate-3 data-[highlighted]:outline-hidden"
    {...props}
  >
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
  </SelectPrimitive.Item>
);
