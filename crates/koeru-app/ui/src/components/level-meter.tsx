import { CLIP_THRESHOLD, TOO_QUIET } from "~/lib/levels";

type LevelMeterProps = {
  /** 0.0〜1.0。 */
  peak: number;
};

/**
 * 入力レベル。
 *
 * `<meter>` を使う（`TR-PLT-29`）。`role="meter"` を付けた `div` と違い、
 * `low` / `high` / `optimum` から支援技術が「よい範囲かどうか」まで拾える。
 * 色分けも `:-moz-meter-*` / `::-webkit-meter-*` が引き受けるので、
 * 同じ判断を JavaScript と CSS の2箇所に書かずに済む。
 *
 * 色だけで伝えない（`TR-PLT-28`）。数値と語も並べる。
 */
export const LevelMeter = ({ peak }: LevelMeterProps) => {
  const pct = Math.min(100, Math.round(peak * 100));
  const state =
    peak >= CLIP_THRESHOLD ? "割れている" : peak < TOO_QUIET ? "小さすぎる" : "ちょうどよい";

  return (
    <div className="flex items-center gap-3">
      <meter
        className="koeru-meter h-2 flex-1"
        min={0}
        max={100}
        low={TOO_QUIET * 100}
        high={CLIP_THRESHOLD * 100}
        optimum={50}
        value={pct}
        aria-label="入力レベル"
      >
        {pct}%
      </meter>
      <span className="w-28 shrink-0 text-right font-mono text-xs text-slate-11 tabular-nums">
        {pct}% {state}
      </span>
    </div>
  );
};
