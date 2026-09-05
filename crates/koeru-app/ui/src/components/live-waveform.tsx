import { useEffect, useRef, useState } from "react";

import { LevelMeter } from "~/components/level-meter";
import { Channel, type EnvelopeView, api } from "~/lib/ipc";

/**
 * いま入ってきている音の波形（`TR-REC-43`）。
 *
 * 録る前から動く。 ストリームは収録画面に入った時点で開いていて（`TR-REC-19`）、
 * 「マイクが拾っているか」は録る前に知りたい。
 *
 * 評価はしない（`TR-REC-16`）。出すのは観測だけで、良し悪しを付けない。
 *
 * アプリが所有する単一の描画面へ直接描く（`TR-PLT-04`）。
 *
 * 包絡は Channel で送られてくる。`invoke` で引きに行かない（`DEC-PLT-017`）。
 * 間隔は Rust 側が刻むので、画面の都合や IPC の混み具合で速度が変わらない。
 */
export const LiveWaveform = () => {
  const ref = useRef<HTMLCanvasElement>(null);
  const data = useRef<[number, number][]>([]);
  /**
   * 波形の色。
   *
   * 毎フレーム `getComputedStyle` を呼ばない。 強制同期スタイル計算が
   * 毎秒 20 回走る。明暗が変わったときだけ読み直す。
   */
  const waveColor = useRef("");
  const [peak, setPeak] = useState(0);

  useEffect(() => {
    let alive = true;

    const draw = () => {
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

      ctx.clearRect(0, 0, w, h);

      const v = data.current;
      const mid = h / 2;

      // 中心線。無音でも「動いている」ことが見える。
      ctx.fillStyle = waveColor.current;
      ctx.globalAlpha = 0.35;
      ctx.fillRect(0, mid - dpr * 0.5, w, Math.max(1, dpr));
      ctx.globalAlpha = 1;
      if (v.length === 0) return;

      /*
       * 1本につき1列。畳まない。
       *
       * 目盛りは 50ms ごとに 10 本ずつ入れ替わる。割り切れない本数へ畳むと、
       * 入れ替わるたびに目盛りと列の対応がずれて**絵が揺れる**——
       * 「速度が一定じゃない」に見える。1本1列なら、10 本ずれれば 10 列ずれるだけ。
       *
       * 計算量は目盛りの本数（300 本、1.5 秒ぶんで固定）に比例する（`TR-PLT-04`）。
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

    // 明暗の切り替えは `<html>` の class で起きる（`public/theme.js`）。
    // そこだけを見て読み直す。
    const readColor = () => {
      waveColor.current = getComputedStyle(document.documentElement)
        .getPropertyValue("--cyan-11")
        .trim();
    };
    readColor();
    const themeWatch = new MutationObserver(readColor);
    themeWatch.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["class"],
    });

    const channel = new Channel<EnvelopeView>();
    channel.onmessage = (frame) => {
      if (!alive) return;
      data.current = frame.steps;
      // Rust 側の Channel から届いた値を反映する。外部の仕組みとの同期。
      // oxlint-disable-next-line react/set-state-in-effect
      setPeak(frame.steps.reduce((m, [lo, hi]) => Math.max(m, -lo, hi), 0));
      draw();
    };

    // 開く前は空で正しい。 騒がない。
    // 番号で名指しして止める。 作り直しのときに、
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
      themeWatch.disconnect();
      alive = false;
      if (generation !== null) api.stopEnvelopeStream(generation).catch(() => {});
    };
  }, []);

  return (
    <div className="flex flex-col gap-3">
      {/*
        同じ購読からメーターも駆動する。 canvas は `role="img"` なので、
        支援技術へ値を届けるのは `<meter>` のほう（TR-PLT-29）。
        別の経路にすると、目に見える波形だけが動いてメーターが止まる。
      */}
      <LevelMeter peak={peak} />
      <canvas
        ref={ref}
        role="img"
        // 毎フレーム書き換えない。 ここは「何の絵か」を言うだけにして、
        // 動く値はメーターが持つ。名前が 20Hz で変わると読み上げが追えない。
        aria-label="いま入っている音の波形"
        className="h-20 w-full rounded-lg bg-slate-3"
      />
    </div>
  );
};
