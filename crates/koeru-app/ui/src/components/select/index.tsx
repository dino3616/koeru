import * as SelectPrimitive from "@radix-ui/react-select";
import type { ComponentProps } from "react";

import { Icon } from "~/components/icon";
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
 * 形は3つ。
 *
 * - `md` — 幅いっぱいに伸びる、フォームの中の選択
 * - `sm` — ツールバーの右端に並ぶ選択（収録順、マイク）。 高さ 28px で、面は `surface-1`
 * - `crumb` — パンくずの中の選択（「ナギ › Normal A3 CVVC ⌄ › 収録」）。 高さ 26px で、
 *   地は透けたまま輪郭だけ引く。 パンくずの字と同じ大きさで並ぶ
 *
 * 寸法は S04 のボードのまま（`DEC-PLT-062`）。 `sm` と `crumb` は指の的としては小さいが、
 * `TR-PLT-31` の 24px は満たす。 既定は `md`。 呼び出し側の見た目を変えない。
 *
 * 中に印を置ける（マイクの札）。 `SelectValue` の前に並べれば、▾ との間と同じだけ空く。
 */
const trigger = tv({
  variants: {
    size: {
      md: "h-11 w-full justify-between gap-2 rounded-lg border-slate-7 bg-slate-3 px-3 text-sm",
      sm: "h-7 w-fit gap-1.5 rounded-md border-line bg-surface-1 px-2.5 text-xs",
      crumb: "h-6.5 w-fit gap-1.5 rounded-md border-line bg-transparent px-2 text-md",
    },
  },
  defaultVariants: { size: "md" },
});

type SelectTriggerProps = Omit<ComponentProps<typeof SelectPrimitive.Trigger>, "className"> &
  VariantProps<typeof trigger>;

export const SelectTrigger = ({ children, size, ...props }: SelectTriggerProps) => (
  <SelectPrimitive.Trigger
    className={`flex items-center border text-1 disabled:opacity-45 data-[placeholder]:text-2 ${trigger({ size })}`}
    {...props}
  >
    {children}
    <SelectPrimitive.Icon className="flex">
      <Icon name="select" size={10} />
    </SelectPrimitive.Icon>
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
