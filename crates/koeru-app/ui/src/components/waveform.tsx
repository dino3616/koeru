import { useEffect, useRef } from "react";

import { cn } from "~/lib/cn";

type WaveformProps = {
  /** バケットごとのピーク（0〜255）。**Rust 側が録音停止時に確定させたもの。** */
  peaks: number[];
  /** 全体のピーク。1.0 に達していたらクリップとして色を変える。 */
  peak: number;
  /** 長さ（ミリ秒）。読み上げ用の説明に使う。 */
  durationMs: number;
  className?: string;
};

/** クリップとみなす閾値。**0.999 以上は歪んでいる。** */
const CLIP_THRESHOLD = 0.999;

/**
 * 波形。
 *
 * **アプリが所有する単一の描画面へ直接描く**（TR-PLT-04）。
 * 標準コントロールを並べて作らない。
 *
 * **見えない人にも状態が伝わるようにする**（TR-PLT-29）。
 * canvas は装飾ではなく情報なので、`role="img"` と説明を付ける。
 */
export const Waveform = ({ peaks, peak, durationMs, className }: WaveformProps) => {
  const ref = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const canvas = ref.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

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

    ctx.clearRect(0, 0, w, h);
    if (peaks.length === 0) return;

    const mid = h / 2;
    ctx.fillStyle = wave;

    // **可視域の画素数に比例した計算量に収める**（TR-PLT-04）。
    // バケットが画素より多ければ、画素ごとに最大値を採ってまとめる。
    const cols = Math.min(w, peaks.length);
    const barW = w / cols;
    for (let i = 0; i < cols; i += 1) {
      const from = Math.floor((i * peaks.length) / cols);
      const to = Math.max(from + 1, Math.floor(((i + 1) * peaks.length) / cols));
      let v = 0;
      for (let j = from; j < to; j += 1) v = Math.max(v, peaks[j] ?? 0);
      // 上下対称に、最低 1px は残す（**無音の位置も見えたほうがよい**）。
      const half = Math.max(dpr * 0.5, (v / 255) * mid);
      ctx.fillRect(i * barW, mid - half, Math.max(1, barW - dpr * 0.5), half * 2);
    }
  }, [peaks, peak]);

  const seconds = (durationMs / 1000).toFixed(2);
  const level = Math.round(peak * 100);
  const label =
    peak >= CLIP_THRESHOLD
      ? `波形。長さ ${seconds} 秒、ピーク ${level} パーセント。音が割れている`
      : `波形。長さ ${seconds} 秒、ピーク ${level} パーセント`;

  return (
    <canvas
      ref={ref}
      role="img"
      aria-label={label}
      className={cn("h-24 w-full rounded-lg bg-surface-2", className)}
    />
  );
};
