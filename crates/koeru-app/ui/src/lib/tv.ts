/*
 * クラス名の組み立て。
 *
 * # tailwind-merge を通さない
 *
 * `tv` は既定で tailwind-merge を通して、衝突したクラスを後勝ちで畳む。
 * これを切ってある。 畳む必要があるのは「外から `className` で上書きされる」
 * 部品だけで、KOERU の部品は `className` を受け取らない（下記）。
 * 受け取らないなら衝突は起きず、畳む処理は毎回の描画で空回りするだけになる。
 *
 * 畳みに頼ると、衝突が黙って解決される。 どちらが勝つかは
 * tailwind-merge の分類表が決めるので、Tailwind の版が上がって分類が変わると、
 * 何も書き換えていないのに見た目が変わる。
 *
 * # 部品は `className` を受け取らない
 *
 * 見た目は部品が持つ。 外から差し込めるようにすると、同じ部品が
 * 呼ばれた場所ごとに違う姿になり、`TR-PLT-28` の対象サイズのような
 * 部品が守っているはずの条件を、呼び出し側が黙って壊せる。
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
import { createTV } from "tailwind-variants";

export { cx } from "tailwind-variants";
export type { VariantProps } from "tailwind-variants";

export const tv = createTV({ twMerge: false });
