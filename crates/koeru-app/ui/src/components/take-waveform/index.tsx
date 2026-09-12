import { useEffect, useRef, useState } from "react";

import { Breath } from "~/components/breath";
import { CLIP_THRESHOLD } from "~/lib/levels";
import { type OtoView, api, errorMessage } from "~/lib/ipc";

type TakeWaveformProps = {
  takeId: number;
  durationMs: number;
  /** 絶対値の最大。[`CLIP_THRESHOLD`] 以上なら割れたものとして扱う。 */
  peak: number;
  /** この回から取れた音。波形の下に帯として並ぶ。 */
  otos: readonly OtoView[];
  /** いま見ている音。帯の1つを濃くする。 */
  selected: string | null;
  /**
   * 帯を選ぶ。渡さなければ帯は読むだけになる。
   *
   * **押せるなら押せる姿に、押せないなら押せない姿にする。** 押せない
   * `<span>` を枠つきで並べていたので、波形の真下という一等地に
   * 「押せそうな枠」が出て、実際の切り替えはその下の小さい札のほうにあった。
   * **同じものを選ぶ的が、そっくりな姿で2つ並んでいた。踏んだ。**
   */
  onSelect?: ((alias: string) => void) | undefined;
  /**
   * 高さ。
   *
   * `sm` は収録の面の右に置くとき。 狭い列に入るので、波形を低くして
   * 音の帯も細くする。読むものは同じ。
   */
  height?: "sm" | "md";
};

/**
 * 切り出して使う区間（ミリ秒）。
 *
 * cutoff は負なら「offset からの長さ」、正なら「ファイル末尾からの距離」。
 * UTAU の慣例で、符号で意味が変わる。
 */
export const usableSpan = (o: OtoView, fileMs: number): [number, number] => {
  const usable = o.cutoff_ms <= 0 ? -o.cutoff_ms : Math.max(0, fileMs - o.offset_ms - o.cutoff_ms);
  return [o.offset_ms, o.offset_ms + usable];
};

/**
 * 録れた音の形（`TR-PLT-04`）。
 *
 * アプリが所有する単一の描画面へ直接描く。標準コントロールを並べて作らない。
 *
 * 可視域のみ計算し、可視域のみ描く。 読む量は表示中の画素数に比例し、
 * 範囲の広さには比例しない。Rust 側が段を積んでいる。
 *
 * **寄る／引くを持たない。** `Q-PLT-004` が閉じるまで、ズーム段の切り替えと
 * 境界をつかんでいるあいだの描き直しは決めない（`DEC-PLT-024`）。
 * 実測前の推測を構造に焼き付けないため、いまは全体だけを描く。
 *
 * 色相を持たない。 波形は道具側の目盛り（`docs/design/direction.md`）。
 * 割れているときだけ red——あれは状態であって装飾ではない。
 */
export const TakeWaveform = ({
  takeId,
  durationMs,
  peak,
  otos,
  selected,
  onSelect,
  height = "md",
}: TakeWaveformProps) => {
  const ref = useRef<HTMLCanvasElement>(null);
  /**
   * 要求ごとの通し番号。
   *
   * 生死の札1つでは足りない。 テイクを続けて切り替えると要求が重なり、
   * 古いほうが後に返ることがある。自分の番号が最新のときだけ描く
   * （`DEC-PLT-017` と同じ理由）。
   */
  const seq = useRef(0);
  const [error, setError] = useState<string | null>(null);
  /**
   * 描く元を取っている最中か。
   *
   * `waveform_window` は WAV を読んで畳むので待つ。 待っている間 canvas は
   * 前の絵のままなので、何も出さないと「変わっていない」と読める。
   */
  const [drawing, setDrawing] = useState(false);

  /*
   * 描くのは effect の中だけ。
   *
   * 関数を切り出して依存に載せない。 `useCallback` を外すと毎描画で
   * 別物になり、`react/exhaustive-deps` が「毎回走る」と正しく指摘する。
   * **`useCallback` で包み直すより、依存を素の値にするほうが読める。**
   */
  useEffect(() => {
    const canvas = ref.current;
    if (canvas === null) return;
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;

    // 論理ピクセルではなく実ピクセルで描く。 そうしないと Retina で滲む。
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const w = Math.max(1, Math.round(rect.width * dpr));
    const h = Math.max(1, Math.round(rect.height * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const styles = getComputedStyle(document.documentElement);
    const wave = styles.getPropertyValue(peak >= CLIP_THRESHOLD ? "--red-11" : "--slate-11").trim();
    const band = styles.getPropertyValue("--slate-4").trim();
    const edge = styles.getPropertyValue("--slate-7").trim();
    /** ミリ秒を画素へ。 */
    const at = (ms: number) => (ms / durationMs) * w;

    seq.current += 1;
    const mine = seq.current;
    setDrawing(true);
    api
      .waveformWindow({ takeId, fromMs: 0, toMs: durationMs, pixels: w })
      .then((points) => {
        // 自分より新しい要求が出ていたら描かない。
        if (mine !== seq.current) return;
        ctx.clearRect(0, 0, w, h);

        // ── 1. 切り出して使う区間を下地に敷く（`TR-ALN-33`）──
        //
        // 波形より先に描く。 上に乗せると波形が隠れる。
        ctx.fillStyle = band;
        for (const o of otos) {
          const [from, to] = usableSpan(o, durationMs);
          ctx.fillRect(at(from), 0, Math.max(dpr, at(to) - at(from)), h);
        }

        // ── 2. 波形 ──
        ctx.fillStyle = wave;
        const mid = h / 2;
        points.forEach(([lo, hi], i) => {
          // 上下対称ではなく、実際の min/max を描く。 非対称な波形が分かる。
          const top = mid - hi * mid;
          const bottom = mid - lo * mid;
          ctx.fillRect(i, top, 1, Math.max(dpr, bottom - top));
        });

        // ── 3. 区間の両端 ──
        ctx.fillStyle = edge;
        for (const o of otos) {
          const [from, to] = usableSpan(o, durationMs);
          ctx.fillRect(at(from), 0, Math.max(dpr, dpr * 1.5), h);
          ctx.fillRect(at(to) - dpr, 0, Math.max(dpr, dpr * 1.5), h);
        }
      })
      .catch((e: unknown) => setError(errorMessage(e)))
      // 失敗しても下ろす。下ろさないと、印が出たまま止まる。
      .finally(() => {
        // 印を下ろすのも最新のものだけ。古い応答が新しい待ちを消さない。
        if (mine === seq.current) setDrawing(false);
      });

    return () => {
      // 番号を進めて、走っている要求の結果を捨てる。
      seq.current += 1;
    };
  }, [takeId, durationMs, peak, otos]);

  const seconds = (durationMs / 1000).toFixed(2);
  const level = Math.round(peak * 100);
  const label =
    peak >= CLIP_THRESHOLD
      ? `録れた音の形。長さ ${seconds} 秒、いちばん大きいところ ${level}%。音が割れている`
      : `録れた音の形。長さ ${seconds} 秒、いちばん大きいところ ${level}%`;

  return (
    <div className="flex flex-col gap-2">
      {/*
        待っている印を canvas の上に重ねる。 入れ替えない——差し替えると
        絵が消えて、待つたびに画面が空白になる。
      */}
      <div className="relative">
        <canvas
          ref={ref}
          role="img"
          aria-label={label}
          className={
            height === "sm"
              ? "h-24 w-full rounded-lg border border-slate-7 bg-slate-1"
              : "h-40 w-full rounded-lg border border-slate-7 bg-slate-1"
          }
        />
        {drawing && (
          <span className="absolute inset-0 flex items-center justify-center rounded-lg bg-slate-1/60 text-slate-11">
            <Breath size="sm" />
          </span>
        )}
      </div>

      {/* 取れた音の帯。1回の録音から取れた音が、同じ絵の上に並ぶ（`DEC-PLT-024`）。 */}
      {otos.length > 0 && (
        <div className={height === "sm" ? "relative h-7" : "relative h-9"}>
          {otos.map((o) => {
            const [from, to] = usableSpan(o, durationMs);
            const box = `absolute flex items-center justify-center overflow-hidden rounded-lg ${
              height === "sm" ? "h-7 text-xs" : "h-9 text-sm"
            }`;
            const place = {
              left: `${(from / durationMs) * 100}%`,
              width: `${((to - from) / durationMs) * 100}%`,
            };
            /*
              読むだけの帯は、枠も hover も持たない。 持たせると押せる的に
              見える——`LastTake` では選ぶ相手がいないので、押しても何も起きない。
            */
            if (onSelect === undefined) {
              return (
                <span key={o.alias} className={`${box} text-slate-11`} style={place}>
                  {o.alias}
                </span>
              );
            }
            return (
              <button
                type="button"
                key={o.alias}
                /*
                  選ばれているかを `aria-pressed` で言う（`TR-PLT-28`）。
                  濃さだけで伝えると、支援技術には「押せるもの」としか届かない。
                */
                aria-pressed={o.alias === selected}
                onClick={() => onSelect(o.alias)}
                /*
                  段は `Chip` と揃える。 **帯の「選ばれていない」に段 4 を
                  使っていたので、`Chip` の「選ばれている」と同じ塗りになっていた。**
                  隣り合う2つの部品で、同じ段が逆の意味を持っていた。**踏んだ。**
                */
                className={`${box} border ${
                  o.alias === selected
                    ? "border-slate-12 bg-slate-4 text-slate-12"
                    : "border-slate-7 bg-transparent text-slate-11 hover:bg-slate-3"
                }`}
                style={place}
              >
                {o.alias}
              </button>
            );
          })}
        </div>
      )}

      <div className="flex justify-between">
        <span className="font-mono text-xs text-slate-11 tabular-nums">0.00 秒</span>
        <span className="font-mono text-xs text-slate-11 tabular-nums">{seconds} 秒</span>
      </div>

      {error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {error}
        </p>
      )}
    </div>
  );
};
