import { useEffect, useRef, useState } from "react";

import { api } from "~/lib/ipc";
import { cn } from "~/lib/cn";

type LiveWaveformProps = {
  /** 描く本数。**画素より多く取らない**（TR-PLT-04）。 */
  buckets?: number;
  /** 引く間隔（ミリ秒）。 */
  intervalMs?: number;
  className?: string;
};

/**
 * いま入ってきている音の波形（TR-REC-43）。
 *
 * **録る前から動く。** ストリームは収録画面に入った時点で開いていて（TR-REC-19）、
 * 「マイクが拾っているか」は録る前に知りたい。
 *
 * **評価はしない**（TR-REC-16）。出すのは観測だけで、良し悪しを付けない。
 *
 * **アプリが所有する単一の描画面へ直接描く**（TR-PLT-04）。
 */
export const LiveWaveform = ({ buckets = 240, intervalMs = 60, className }: LiveWaveformProps) => {
  const ref = useRef<HTMLCanvasElement>(null);
  const data = useRef<[number, number][]>([]);
  const [peak, setPeak] = useState(0);

  useEffect(() => {
    let alive = true;
    const t = window.setInterval(() => {
      api
        .liveEnvelope(buckets)
        .then((v) => {
          if (!alive) return;
          data.current = v;
          setPeak(v.reduce((m, [lo, hi]) => Math.max(m, -lo, hi), 0));
          draw();
        })
        .catch(() => {
          // **止まっていることを騒がない。** ストリームが開く前は空で正しい。
          if (alive) {
            data.current = [];
            draw();
          }
        });
    }, intervalMs);
    return () => {
      alive = false;
      window.clearInterval(t);
    };
  }, [buckets, intervalMs]);

  const draw = () => {
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
    ctx.clearRect(0, 0, w, h);

    const v = data.current;
    const mid = h / 2;

    // 中心線。**無音でも「動いている」ことが見える。**
    ctx.fillStyle = styles.getPropertyValue("--wave").trim();
    ctx.globalAlpha = 0.35;
    ctx.fillRect(0, mid - dpr * 0.5, w, Math.max(1, dpr));
    ctx.globalAlpha = 1;
    if (v.length === 0) return;

    // **可視域の画素数に比例した計算量に収める**（TR-PLT-04）。
    const cols = Math.min(w, v.length);
    const barW = w / cols;
    for (let i = 0; i < cols; i += 1) {
      const from = Math.floor((i * v.length) / cols);
      const to = Math.max(from + 1, Math.floor(((i + 1) * v.length) / cols));
      let lo = 0;
      let hi = 0;
      for (let j = from; j < to; j += 1) {
        const b = v[j];
        if (!b) continue;
        lo = Math.min(lo, b[0]);
        hi = Math.max(hi, b[1]);
      }
      const top = mid - hi * mid;
      const bottom = mid - lo * mid;
      ctx.fillRect(i * barW, top, Math.max(1, barW - dpr * 0.5), Math.max(dpr, bottom - top));
    }
  };

  const level = Math.round(peak * 100);
  return (
    <canvas
      ref={ref}
      role="img"
      aria-label={`いま入っている音の波形。レベル ${level} パーセント`}
      className={cn("h-20 w-full rounded-lg bg-surface-2", className)}
    />
  );
};
