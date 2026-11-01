import type { CSSProperties } from "react";

import { FLOOR_DBFS, formatDbfs, toDbfs, toPercent } from "./scale";

type InputLevelProps = {
  /** いま入っている大きさ（0.0〜1.0）。 バーの長さ。 */
  level: number;
  /** 直前のいちばん大きいところ（0.0〜1.0）。 縦線の位置と、右端の値。 */
  peak: number;
  /** 割れた回数。押しつけない——起きた回数を数えるだけ。 */
  clipped: number;
};

/**
 * 入っている音（`TR-REC-16`、`DEC-REC-008`、`Q-REC-004`）。
 *
 * 判定しない。 「ちょうどよい」「小さすぎる」を出さない。
 * 確定したテイクには生の数値しか出さないのに、鳴っている音だけ判定される
 * ——同じ画面の中でその食い違いを作らない。
 *
 * バーは単色で、緑・黄・赤の帯を持たない。 色で「割れそう」を伝えると、
 * 上の判定しないことと食い違う。 伝えるのは右端の説明と、数値（dBFS）と、
 * 割れた回数の3つ。
 *
 * `<meter>` を使う（`TR-PLT-29`）。`role="meter"` を付けた `div` と違い、
 * 値と範囲が最初から支援技術へ届く。区分（`low` / `high` / `optimum`）は
 * 持たせない——持たせると「よい範囲かどうか」が届いてしまう。
 *
 * 色だけで伝えない（`TR-PLT-28`）。 数値も並べる。
 *
 * 「いちばん右まで届くと、音が割れます」は評価ではない。 起きることの説明として置く。
 */
export const InputLevel = ({ level, peak, clipped }: InputLevelProps) => {
  // 位置は音ごとに変わる。 変わる値は変数で渡し、当てるのはクラス。
  const place = { "--peak-at": `${toPercent(peak)}%` } as CSSProperties;
  return (
    <div className="flex flex-col gap-2">
      <div className="flex justify-between text-xs text-slate-11">
        <span>入力</span>
        <span className="font-mono tabular-nums">ピーク {formatDbfs(peak)} dBFS</span>
      </div>

      <div className="relative">
        <meter
          className="koeru-meter block h-3 w-full"
          min={FLOOR_DBFS}
          max={0}
          value={Math.max(FLOOR_DBFS, toDbfs(level))}
          aria-label="入っている音の大きさ"
          aria-valuetext={`${formatDbfs(level)} dBFS`}
        >
          {formatDbfs(level)} dBFS
        </meter>
        {/* 読み上げには数値が届いている。 線は見える人のための印。 */}
        <span
          aria-hidden="true"
          className="absolute inset-y-0 left-(--peak-at) w-0.5 -translate-x-1/2 bg-slate-11"
          style={place}
        />
      </div>

      <p className="text-xs text-slate-11">
        いちばん右まで届くと、音が割れます。縦線は直前のピーク。
      </p>
      <p className="text-xs text-slate-11">
        割れた回数 <span className="font-mono text-slate-12 tabular-nums">{clipped} 回</span>
      </p>
    </div>
  );
};
