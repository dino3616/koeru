import { Slot } from "@radix-ui/react-slot";
import type { ComponentProps } from "react";

import { type VariantProps, cn, tv } from "~/lib/tv";

/**
 * 見た目の切り替え。
 *
 * `tv` に入れるのは値で変わるものだけ。 静的なクラスは下の JSX に直接書く
 * ——`base` に入れると、見た目を読むのに2箇所を行き来することになる。
 *
 * 押せるものに色相を与えない（`docs/reports/ux/direction.md`）。 色相を持って
 * よいのは声と状態色（red / amber）だけなので、「押せる」を色で言わず、
 * 塗りと輪郭の有無で言う。
 *
 * - `primary` — 主。 1画面に1つ。 声の色で塗り、字は `voice-ink`。 声が無い音源では
 *   slate 11 に落ちる（`DEC-PLT-062`、塗りの式は `globals.css` の `bg-primary`）
 * - `secondary` — 副。 輪郭だけ
 * - `ghost` — 小。 地に溶けて、hover で面が出るだけ
 * - `quiet` — `ghost` の字を `text-2` に落としたもの。 ツールバーの開閉や履歴のように、
 *   いつもそこにあって目で探さない印のボタン
 * - `raised` — 面に `surface-2` を敷き、`line` で縁取る。 収録の操作の丸（先頭に戻す・再生）
 * - `danger` — red で塗る。 主・副・小の3種の外。 録音を止める操作が使っている
 *
 * 押している間は、面を1段進める。 `danger` は塗りが段 12 で行き止まりなので進めない。
 *
 * 太さも variant が持つ。 主だけ `font-black` で、ほかは `font-semibold`。 base に
 * 置くと、どちらが効くかを `cn` の畳みの向きに預けることになる。
 */
const button = tv({
  variants: {
    variant: {
      primary:
        "bg-primary font-black text-voice-ink hover:bg-primary-hover active:bg-primary-active",
      secondary:
        "border border-line-strong font-semibold text-1 hover:bg-surface-2 active:bg-surface-3",
      ghost: "font-semibold text-1 hover:bg-surface-2 active:bg-surface-3",
      quiet: "font-semibold text-2 hover:bg-surface-2 hover:text-1 active:bg-surface-3",
      raised: "border border-line bg-surface-2 font-semibold text-1 hover:bg-surface-3",
      danger: "bg-red-11 font-semibold text-slate-1 hover:bg-red-12",
    },
    size: {
      // 高さは 44px 以上を既定にする（`TR-PLT-31` の操作対象の大きさ）。
      // 収録中は画面を見ずに押すことがあるので、小さい的にしない。
      md: "h-11 px-5 text-sm",
      /*
       * 一覧の行に並ぶもの。36px で、指の的としては小さい。
       *
       * 使ってよいのは「同じことが別の大きい的からもできる」場所だけ。
       * 行ごとの「聴く」は、そのテイクを開けば大きい的で聴ける。
       */
      sm: "h-9 px-3 text-sm",
      /*
       * 幅と高さを分けて書く。 `size-11` にすると、外から `w-full` が来ても
       * 畳まれない——畳む側の分類では `size` が `w` を上書きする向きしか無く、
       * 逆向きは衝突として扱われない。 どちらが効くかが生成 CSS の順序で決まる。
       */
      icon: "h-11 w-11",
      /*
       * 印だけのボタンの小さいもの。 寸法は S04 のボードのまま（`DEC-PLT-062`）。
       *
       * どれも `TR-PLT-31` の 24px を的の大きさか間隔で満たす。 20px の `icon-2xs` は、
       * テイクの切り替えの ⋮ のように、隣の的から中心が 24px 以上離れる所にだけ置く。
       */
      "icon-lg": "h-13 w-13 rounded-full",
      "icon-sm": "h-7 w-7 rounded-md",
      "icon-xs": "h-6 w-6 rounded-md",
      "icon-2xs": "h-5 w-5 rounded-md",
    },
  },
  defaultVariants: { variant: "secondary", size: "md" },
});

type ButtonProps = ComponentProps<"button"> &
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
 * `className` で入れてよいのは、置く側が決めるもの（幅と、flex の中での
 * 振る舞い）だけ。 大きさと配色は `variant` / `size` で選ぶ——高さを
 * 差し込めるようにすると、`TR-PLT-31` の操作対象の大きさを呼び出し側が壊せる。
 * 何を通すかは `shadcn/no-restyle` の contract が持つ（`vite.config.ts`）。
 *
 * 色の遷移を持たない。 道具側は動かない（`docs/reports/ux/direction.md`）ので、
 * hover も即座に切り替わる。
 */
export const Button = ({ variant, size, asChild = false, className, ...props }: ButtonProps) => {
  const Comp = asChild ? Slot : "button";
  return (
    <Comp
      className={cn(
        "inline-flex items-center justify-center gap-2 rounded-lg disabled:pointer-events-none disabled:opacity-45",
        button({ variant, size }),
        className,
      )}
      {...props}
    />
  );
};
