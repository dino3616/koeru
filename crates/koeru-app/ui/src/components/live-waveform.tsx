import { useEffect, useRef, useState } from "react";

import { cn } from "~/lib/cn";
import { Channel, type EnvelopeView, api } from "~/lib/ipc";

type LiveWaveformProps = {
  /** 描く本数。**画素より多く取らない**（TR-PLT-04）。 */
  buckets?: number;
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
 *
 * # 引きに行かず、送ってもらう
 *
 * **Tauri は streaming に Channel を使えと言っている**——
 * 「Channels are designed to be fast and deliver ordered data」。
 * `invoke` で引きに行くと **応答が投げた順に返る保証が無く**、
 * 1回が間隔より長くかかると問い合わせが重なって、
 * 遅れて届いた古い包絡で**波形が巻き戻る**（「ループする」）。
 * event（`emit`/`listen`）も「not designed for low latency or high throughput」
 * と明記されているので、これも違う。
 *
 * 間隔は Rust 側が刻む。**画面の都合や IPC の混み具合で速度が変わらない。**
 */
export const LiveWaveform = ({ buckets = 240, className }: LiveWaveformProps) => {
  const ref = useRef<HTMLCanvasElement>(null);
  const data = useRef<[number, number][]>([]);
  const [peak, setPeak] = useState(0);

  useEffect(() => {
    let alive = true;

    const draw = () => {
      const canvas = ref.current;
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
          if (b === undefined) continue;
          lo = Math.min(lo, b[0]);
          hi = Math.max(hi, b[1]);
        }
        const top = mid - hi * mid;
        const bottom = mid - lo * mid;
        ctx.fillRect(i * barW, top, Math.max(1, barW - dpr * 0.5), Math.max(dpr, bottom - top));
      }
    };

    const channel = new Channel<EnvelopeView>();
    channel.onmessage = (frame) => {
      if (!alive) return;
      data.current = frame.buckets;
      setPeak(frame.buckets.reduce((m, [lo, hi]) => Math.max(m, -lo, hi), 0));
      draw();
    };

    // **開く前は空で正しい。** 騒がない。
    api.streamEnvelope(buckets, channel).catch(() => {});

    return () => {
      alive = false;
      api.stopEnvelopeStream().catch(() => {});
    };
  }, [buckets]);

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
