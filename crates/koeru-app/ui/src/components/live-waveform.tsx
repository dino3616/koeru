import { useEffect, useRef, useState } from "react";

import { cn } from "~/lib/cn";
import { Channel, type EnvelopeView, api } from "~/lib/ipc";

type LiveWaveformProps = {
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
export const LiveWaveform = ({ className }: LiveWaveformProps) => {
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

      /*
       * **1本につき1列。畳まない。**
       *
       * 目盛りは 50ms ごとに 10 本ずつ入れ替わる。割り切れない本数へ畳むと、
       * 入れ替わるたびに目盛りと列の対応がずれて**絵が揺れる**——
       * 「速度が一定じゃない」に見える。1本1列なら、10 本ずれれば 10 列ずれるだけ。
       *
       * 計算量は目盛りの本数（300 本、1.5 秒ぶんで固定）に比例する（TR-PLT-04）。
       */
      const barW = w / v.length;
      for (let i = 0; i < v.length; i += 1) {
        const b = v[i];
        if (b === undefined) continue;
        const top = mid - b[1] * mid;
        const bottom = mid - b[0] * mid;
        ctx.fillRect(i * barW, top, Math.max(1, barW), Math.max(dpr, bottom - top));
      }
    };

    const channel = new Channel<EnvelopeView>();
    channel.onmessage = (frame) => {
      if (!alive) return;
      data.current = frame.steps;
      setPeak(frame.steps.reduce((m, [lo, hi]) => Math.max(m, -lo, hi), 0));
      draw();
    };

    // **開く前は空で正しい。** 騒がない。
    // **番号で名指しして止める。** 作り直しのときに、
    // 「古いのを止める」より「新しいのを始める」が先に着くことがある。
    let generation: number | null = null;
    api
      .streamEnvelope(channel)
      .then((g) => {
        generation = g;
        // 番号が返る前に外されていたら、ここで止める。
        if (!alive) api.stopEnvelopeStream(g).catch(() => {});
      })
      .catch(() => {});

    return () => {
      alive = false;
      if (generation !== null) api.stopEnvelopeStream(generation).catch(() => {});
    };
  }, []);

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
