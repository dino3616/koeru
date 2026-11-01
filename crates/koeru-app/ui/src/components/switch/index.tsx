import { type ComponentProps, useId } from "react";

import { tv } from "~/lib/tv";

/*
 * 入っているかは、つまみの位置と軌道の塗りで言う。 色相を持たない。
 * 切っているときの軌道は塗らずに段 11 の縁だけ、入っているときは段 11 で塗る。
 */
const track = tv({
  variants: {
    checked: {
      true: "border-slate-11 bg-slate-11",
      false: "border-slate-11 bg-transparent",
    },
  },
});

const knob = tv({
  variants: {
    checked: {
      true: "translate-x-6 bg-slate-1",
      false: "translate-x-1 bg-slate-11",
    },
  },
});

type SwitchProps = Omit<
  ComponentProps<"button">,
  | "className"
  | "type"
  | "role"
  | "onClick"
  | "aria-checked"
  | "aria-labelledby"
  | "aria-describedby"
  | "children"
> & {
  /** 見えている名札。名前は `aria-label` にせず、この字から読ませる。 */
  label: string;
  /** 名札の下に置く一行。何が起きるかを書く。 */
  description?: string;
  checked: boolean;
  onCheckedChange: (next: boolean) => void;
};

/**
 * 入り切りの札。
 *
 * `role="switch"` と `aria-checked` を持つ（`TR-PLT-28`）。 `Chip` の
 * `aria-pressed` は「絞り込みの札」で、入り切りの意味は持たない——
 * 同意のように「いまオンか」を読ませたいものは、こちらを使う。
 *
 * 押せる範囲は 44px 四方以上にする（`TR-PLT-31`）。 見えている軌道はそれより
 * 小さいので、的は外側の `<button>` が持ち、軌道は中に描く。
 *
 * 名前と説明は、隣の字から `aria-labelledby` / `aria-describedby` で引く。
 * 説明まで名前に混ざると、スイッチの名前が長い文になる。
 */
export const Switch = ({ label, description, checked, onCheckedChange, ...props }: SwitchProps) => {
  const labelId = useId();
  const descriptionId = useId();
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex min-w-0 flex-col gap-1">
        <span id={labelId} className="text-sm text-slate-12">
          {label}
        </span>
        {description !== undefined && (
          <span id={descriptionId} className="text-xs text-slate-11">
            {description}
          </span>
        )}
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-labelledby={labelId}
        {...(description === undefined ? {} : { "aria-describedby": descriptionId })}
        onClick={() => onCheckedChange(!checked)}
        className="inline-flex h-11 w-14 flex-shrink-0 items-center justify-center rounded-lg disabled:pointer-events-none disabled:opacity-45"
        {...props}
      >
        <span
          aria-hidden="true"
          className={`flex h-6 w-11 items-center rounded-full border ${track({ checked })}`}
        >
          <span className={`h-4 w-4 rounded-full ${knob({ checked })}`} />
        </span>
      </button>
    </div>
  );
};
