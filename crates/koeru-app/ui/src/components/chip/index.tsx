import type { ComponentProps } from "react";

type ChipProps = Omit<ComponentProps<"button">, "className"> & {
  /** 選ばれているか。絞り込みの札は必ず持つ。 */
  pressed: boolean;
};

/**
 * 絞り込みと切り替えの札。
 *
 * `aria-pressed` を必ず持つ。 見た目の濃さだけで選択を伝えると、
 * 支援技術には「押せるもの」としか届かない（`TR-PLT-28`）。
 *
 * 色相を持たない。 選ばれているかは面の段（3 → 4）と字の段（11 → 12）で言う。
 *
 * 36px にしてある。 `Button` の `sm` と同じで、指の的としては小さい。
 * 一覧の上に並ぶ絞り込みは、押し損ねても壊れる操作が無い場所にだけ置く。
 */
export const Chip = ({ pressed, ...props }: ChipProps) => (
  <button
    type="button"
    aria-pressed={pressed}
    className={`inline-flex h-9 items-center gap-2 rounded-lg border border-slate-7 px-3 text-xs ${
      pressed ? "bg-slate-4 text-slate-12" : "bg-transparent text-slate-11 hover:bg-slate-3"
    }`}
    {...props}
  />
);
