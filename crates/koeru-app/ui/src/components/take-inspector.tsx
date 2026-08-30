import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "~/components/ui/button";
import { cn } from "~/lib/cn";
import { api, errorMessage } from "~/lib/ipc";

type TakeInspectorProps = {
  takeId: number;
  durationMs: number;
  /** 1.0 に達していたらクリップとして色を変える。 */
  peak: number;
  className?: string;
};

/** クリップとみなす閾値。 */
const CLIP_THRESHOLD = 0.999;
/** スペクトログラムの高さ（周波数方向の段数）。 */
const SPECTRO_ROWS = 96;
/** 引いたときの1画面の最短（ミリ秒）。**これ以上は寄れない。** */
const MIN_SPAN_MS = 20;

/**
 * 録れたテイクの波形とスペクトログラム（TR-PLT-04）。
 *
 * **アプリが所有する単一の描画面へ直接描く。**
 * 標準コントロールを並べて作らない。
 *
 * **可視域のみ計算し、可視域のみ描く。** ズーム倍率が変わっても、
 * 読む量は表示中の画素数に比例する——範囲の広さには比例しない。
 * Rust 側が段を積んでいるので、引いても寄っても同じ量しか渡ってこない。
 *
 * **素材全体の STFT を先に計算しない。** スペクトログラムも見えている範囲だけ。
 */
export const TakeInspector = ({ takeId, durationMs, peak, className }: TakeInspectorProps) => {
  const waveRef = useRef<HTMLCanvasElement>(null);
  const spectroRef = useRef<HTMLCanvasElement>(null);
  const [span, setSpan] = useState<[number, number]>([0, durationMs]);
  const [showSpectro, setShowSpectro] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // テイクが変わったら全体へ戻す。
  useEffect(() => {
    setSpan([0, durationMs]);
  }, [durationMs]);

  const draw = useCallback(() => {
    const canvas = waveRef.current;
    if (canvas === null) return;
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;

    // **論理ピクセルではなく実ピクセルで描く。** そうしないと Retina で滲む。
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const w = Math.max(1, Math.round(rect.width * dpr));
    const h = Math.max(1, Math.round(rect.height * dpr));
    if (canvas.width !== w || canvas.height !== h) {
      canvas.width = w;
      canvas.height = h;
    }

    const styles = getComputedStyle(document.documentElement);
    const clipped = peak >= CLIP_THRESHOLD;
    const wave = styles.getPropertyValue(clipped ? "--wave-clip" : "--wave").trim();

    api
      .waveformWindow(takeId, span[0], span[1], w)
      .then((points) => {
        ctx.clearRect(0, 0, w, h);
        ctx.fillStyle = wave;
        const mid = h / 2;
        points.forEach(([lo, hi], i) => {
          // **上下対称ではなく、実際の min/max を描く。** 非対称な波形が分かる。
          const top = mid - hi * mid;
          const bottom = mid - lo * mid;
          ctx.fillRect(i, top, 1, Math.max(dpr, bottom - top));
        });
      })
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [takeId, span, peak]);

  const drawSpectro = useCallback(() => {
    const canvas = spectroRef.current;
    if (canvas === null || !showSpectro) return;
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;

    const rect = canvas.getBoundingClientRect();
    // **列は画素より粗くてよい。** 引き伸ばして描く。
    const columns = Math.max(1, Math.min(256, Math.round(rect.width / 3)));
    canvas.width = columns;
    canvas.height = SPECTRO_ROWS;

    api
      .spectrogramWindow(takeId, span[0], span[1], columns, SPECTRO_ROWS)
      .then((s) => {
        const image = ctx.createImageData(s.columns, s.rows);
        for (let c = 0; c < s.columns; c += 1) {
          for (let r = 0; r < s.rows; r += 1) {
            // 下が低い周波数になるよう、上下を返す。
            const v = s.bins[c * s.rows + (s.rows - 1 - r)] ?? 0;
            const at = (r * s.columns + c) * 4;
            // 暗い藍から明るい水色へ。**単色の濃淡より段が読める。**
            image.data[at] = Math.round(v * 0.35);
            image.data[at + 1] = Math.round(v * 0.85);
            image.data[at + 2] = Math.round(60 + v * 0.75);
            image.data[at + 3] = 255;
          }
        }
        ctx.putImageData(image, 0, 0);
      })
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [takeId, span, showSpectro]);

  useEffect(draw, [draw]);
  useEffect(drawSpectro, [drawSpectro]);

  const zoom = (factor: number) => {
    setSpan(([from, to]) => {
      const centre = (from + to) / 2;
      const half = Math.max(MIN_SPAN_MS / 2, ((to - from) / 2) * factor);
      return [Math.max(0, centre - half), Math.min(durationMs, centre + half)];
    });
  };

  const seconds = (durationMs / 1000).toFixed(2);
  const level = Math.round(peak * 100);
  const label =
    peak >= CLIP_THRESHOLD
      ? `波形。長さ ${seconds} 秒、ピーク ${level} パーセント。音が割れている`
      : `波形。長さ ${seconds} 秒、ピーク ${level} パーセント`;

  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <canvas
        ref={waveRef}
        role="img"
        aria-label={label}
        className="h-24 w-full rounded-lg bg-surface-2"
      />

      {showSpectro && (
        <canvas
          ref={spectroRef}
          role="img"
          aria-label={`スペクトログラム。${(span[0] / 1000).toFixed(2)} 秒から ${(span[1] / 1000).toFixed(2)} 秒`}
          className="h-32 w-full rounded-lg bg-surface-2"
          style={{ imageRendering: "pixelated" }}
        />
      )}

      <div className="flex flex-wrap items-center gap-2">
        <Button variant="ghost" onClick={() => zoom(0.5)} aria-label="拡大">
          ＋
        </Button>
        <Button variant="ghost" onClick={() => zoom(2)} aria-label="縮小">
          −
        </Button>
        <Button variant="ghost" onClick={() => setSpan([0, durationMs])}>
          全体
        </Button>
        <Button variant="ghost" onClick={() => setShowSpectro((v) => !v)}>
          {showSpectro ? "スペクトログラムを隠す" : "スペクトログラム"}
        </Button>
        <span className="font-mono text-xs text-text-dim tabular-nums">
          {(span[0] / 1000).toFixed(2)} – {(span[1] / 1000).toFixed(2)} 秒
        </span>
      </div>

      {error !== null && (
        <p role="alert" className="text-sm text-danger-text">
          {error}
        </p>
      )}
    </div>
  );
};
