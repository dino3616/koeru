/*
 * クラス名の組み立て。
 *
 * # 変わるものは `tv`、外から来たものは `cn`
 *
 * `tv` は `tailwind-variants/lite` から取る。 畳む実装を持たない入口で、
 * variants は同じ軸の中で排他なので、畳む相手がそもそもいない。
 * 既定の入口は畳む実装を分類表ごと抱えていて、切っても束に載ったまま残る。
 *
 * 衝突しうるのは、外から `className` で入ったものと部品が持つクラスだけ。
 * そこは `cn` が後勝ちで畳む。 効かないなら受け取る意味が無いので、
 * 呼び出し側が書いたものを勝たせる。 畳むのはこの1箇所だけにする。
 *
 * # 何を注入してよいかは lint が決める
 *
 * 型で言えるのは「受け取るか否か」までで、「幅は良いが高さは駄目」が書けない。
 * 部品が守っている条件（`Button` の高さは `TR-PLT-31` の操作対象の大きさ）を
 * 呼び出し側が壊せないように、注入してよいクラスは `shadcn/no-restyle` の
 * contract で列挙する（`vite.config.ts`）。 そこに無いものは lint が落とす。
 *
 * 余白は置く側が持つ。 `<LiveWaveform className="mt-3" />` ではなく、
 * 置く側の `flex` / `gap` で空ける。
 *
 * # `tv` は要るところにだけ
 *
 * 静的なクラスは JSX にそのまま書く。 `base` に移さない——
 * 見た目を読むのに2箇所を行き来することになる。
 * `tv` を通すのは、値で切り替わるもの（variants と compound）だけ。
 */
export { cn } from "cn";
export { cx, tv } from "tailwind-variants/lite";
export type { VariantProps } from "tailwind-variants/lite";
