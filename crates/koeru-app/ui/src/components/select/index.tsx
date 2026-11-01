import * as SelectPrimitive from "@radix-ui/react-select";
import type { ComponentProps } from "react";

import { type VariantProps, tv } from "~/lib/tv";

/*
 * 一覧から1つ選ぶ。
 *
 * Radix Primitives のまま出す（`DEC-PLT-015`）。 フォーカス管理と
 * 型入力での移動、`aria-*` の付け外しはあちらが持っている。
 *
 * どれも `className` を受け取らない。 見た目は部品が持つ（`~/lib/tv` の冒頭）。
 */

/*
 * `Root` と `Value` も包む。
 *
 * そのまま再輸出すると Radix の `className` が props に出て、
 * 「どれも `className` を受け取らない」（`DEC-PLT-020`）に反する。
 * 見た目を持たない部品でも、口が開いていれば差し込まれる。
 */
export const Select = (props: Omit<ComponentProps<typeof SelectPrimitive.Root>, "className">) => (
  <SelectPrimitive.Root {...props} />
);

export const SelectValue = (
  props: Omit<ComponentProps<typeof SelectPrimitive.Value>, "className">,
) => <SelectPrimitive.Value {...props} />;

/*
 * 大きさは2つ。
 *
 * `md` は幅いっぱいに伸びる、フォームの中の選択。 `sm` は中身の幅で収まる、
 * ツールバーのパンくずの中の選択（「ナギ › Normal A3 CVVC ⌄ › 収録」）。
 * 高さは 36px で、ツールバーの高さ（`--height-toolbar`）に余白を持って収まる。
 * `Button` の `sm` と同じく指の的としては小さいが、同じことが別の場所からもできる。
 * 既定は `md`。 呼び出し側の見た目を変えない。
 */
const trigger = tv({
  variants: {
    size: {
      md: "h-11 w-full gap-2 px-3 text-sm",
      sm: "h-9 w-fit gap-1 px-2 text-md",
    },
  },
  defaultVariants: { size: "md" },
});

type SelectTriggerProps = Omit<ComponentProps<typeof SelectPrimitive.Trigger>, "className"> &
  VariantProps<typeof trigger>;

export const SelectTrigger = ({ children, size, ...props }: SelectTriggerProps) => (
  <SelectPrimitive.Trigger
    className={`flex items-center justify-between rounded-lg border border-slate-7 bg-slate-3 text-slate-12 disabled:opacity-45 data-[placeholder]:text-slate-11 ${trigger({ size })}`}
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
      className="z-50 min-w-(--radix-select-trigger-width) overflow-hidden rounded-lg border border-slate-7 bg-slate-2 p-1"
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
    className="flex h-10 cursor-default select-none items-center rounded-md px-3 text-sm text-slate-12 data-[highlighted]:bg-slate-3 data-[highlighted]:outline-hidden"
    {...props}
  >
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
  </SelectPrimitive.Item>
);
