type InputLevelProps = {
  /** いちばん大きいところ（0.0〜1.0）。 */
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
 * `<meter>` を使う（`TR-PLT-29`）。`role="meter"` を付けた `div` と違い、
 * 値と範囲が最初から支援技術へ届く。区分（`low` / `high` / `optimum`）は
 * 持たせない——持たせると「よい範囲かどうか」が届いてしまう。
 *
 * 色だけで伝えない（`TR-PLT-28`）。 数値も並べる。
 *
 * 「いちばん右まで届くと、音が割れます」は評価ではない。 起きることの説明として置く。
 */
export const InputLevel = ({ peak, clipped }: InputLevelProps) => {
  const pct = Math.min(100, Math.round(peak * 100));
  return (
    <div className="flex flex-col gap-2">
      <meter
        className="koeru-meter h-3 w-full"
        min={0}
        max={100}
        value={pct}
        aria-label="入っている音の大きさ"
      >
        {pct}%
      </meter>

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 font-mono text-xs text-slate-11 tabular-nums">
        <dt>いちばん大きいところ</dt>
        <dd className="m-0 text-slate-12">{pct}%</dd>
        <dt>割れた回数</dt>
        <dd className="m-0 text-slate-12">{clipped} 回</dd>
      </dl>

      <p className="text-xs text-slate-11">いちばん右まで届くと、音が割れます。</p>
    </div>
  );
};
