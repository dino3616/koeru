import { Slot } from "@radix-ui/react-slot";
import type { ComponentProps } from "react";

import { type VariantProps, tv } from "~/lib/tv";

/**
 * 見た目の切り替え。
 *
 * `tv` に入れるのは値で変わるものだけ。 静的なクラスは下の JSX に直接書く
 * ——`base` に入れると、見た目を読むのに2箇所を行き来することになる。
 *
 * 押せるものに色相を与えない（`docs/design/direction.md`）。 塗りは段 11、
 * 字は段 1。色相を持ってよいのは声と状態色（red / amber）だけなので、
 * 「押せる」を色で言わず、塗りの有無で言う。
 */
const button = tv({
  variants: {
    variant: {
      primary: "bg-slate-11 text-slate-1 hover:bg-slate-12",
      secondary: "border border-slate-7 bg-slate-3 text-slate-12 hover:bg-slate-4",
      ghost: "text-slate-11 hover:bg-slate-3 hover:text-slate-12",
      danger: "bg-red-11 text-slate-1 hover:bg-red-12",
    },
    size: {
      // 高さは 44px 以上を既定にする（`TR-PLT-28` の対象サイズ）。
      // 収録中は画面を見ずに押すことがあるので、小さい的にしない。
      md: "h-11 px-5 text-sm",
      /*
       * 一覧の行に並ぶもの。36px で、指の的としては小さい。
       *
       * 使ってよいのは「同じことが別の大きい的からもできる」場所だけ。
       * 行ごとの「聴く」は、そのテイクを開けば大きい的で聴ける。
       */
      sm: "h-9 px-3 text-sm",
      icon: "size-11",
    },
  },
  defaultVariants: { variant: "secondary", size: "md" },
});

type ButtonProps = Omit<ComponentProps<"button">, "className"> &
  VariantProps<typeof button> & {
    /** 別の要素として描く（リンクなど）。 */
    asChild?: boolean;
  };

/**
 * 押せるもの。
 *
 * キーボードだけで到達でき、閉じ込められない（`TR-PLT-26`）。
 * 素の `<button>` のまま出すので、Tab の順序も Enter / Space も既定のまま効く。
 *
 * `className` は受け取らない。 大きさと配色は `variant` / `size` で選ぶ。
 * 外から差し込めるようにすると、`TR-PLT-28` の対象サイズを呼び出し側が壊せる。
 *
 * 色の遷移を持たない。 道具側は動かない（`docs/design/direction.md`）ので、
 * hover も即座に切り替わる。
 */
export const Button = ({ variant, size, asChild = false, ...props }: ButtonProps) => {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      className={`inline-flex items-center justify-center gap-2 rounded-lg font-semibold disabled:pointer-events-none disabled:opacity-45 ${button({ variant, size })}`}
      {...props}
    />
  );
};
