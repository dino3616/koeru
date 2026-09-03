import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "~/components/ui/button";
import { cn } from "~/lib/cn";
import { type OtoView, api, errorMessage } from "~/lib/ipc";

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
 * 切り出して使う区間（ミリ秒）。
 *
 * **cutoff は負なら「offset からの長さ」、正なら「ファイル末尾からの距離」。**
 * UTAU の慣例で、符号で意味が変わる。
 */
const usableSpan = (o: OtoView, fileMs: number): [number, number] => {
  const usable = o.cutoff_ms <= 0 ? -o.cutoff_ms : Math.max(0, fileMs - o.offset_ms - o.cutoff_ms);
  return [o.offset_ms, o.offset_ms + usable];
};

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
  const [otos, setOtos] = useState<OtoView[]>([]);
  const [error, setError] = useState<string | null>(null);

  // **自動原音設定が指した位置**（TR-ALN-33）。テイクが変わったら引き直す。
  useEffect(() => {
    api
      .otosOfTake(takeId)
      .then(setOtos)
      .catch(() => setOtos([]));
  }, [takeId]);

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

    const boundary = styles.getPropertyValue("--boundary").trim();
    const band = styles.getPropertyValue("--boundary-surface").trim();
    /** ミリ秒を画素へ。 */
    const at = (ms: number) => ((ms - span[0]) / (span[1] - span[0])) * w;

    api
      .waveformWindow(takeId, span[0], span[1], w)
      .then((points) => {
        ctx.clearRect(0, 0, w, h);

        // ── 1. 切り出して使う区間を下地に敷く（TR-ALN-33）──
        //
        // **波形より先に描く。** 上に乗せると波形が隠れる。
        ctx.fillStyle = band;
        for (const o of otos) {
          const [from, to] = usableSpan(o, durationMs);
          ctx.fillRect(at(from), 0, Math.max(dpr, at(to) - at(from)), h);
        }

        // ── 2. 波形 ──
        ctx.fillStyle = wave;
        const mid = h / 2;
        points.forEach(([lo, hi], i) => {
          // **上下対称ではなく、実際の min/max を描く。** 非対称な波形が分かる。
          const top = mid - hi * mid;
          const bottom = mid - lo * mid;
          ctx.fillRect(i, top, 1, Math.max(dpr, bottom - top));
        });

        // ── 3. 境界と、その意味（TR-ALN-33）──
        //
        // **数字だけでは、発声と重なっているかが分からない。**
        // 4モーラが 100ms に潰れていても「確信度 30%」としか出なかった。
        ctx.font = `${Math.round(11 * dpr)}px ui-monospace, monospace`;
        ctx.textBaseline = "top";
        for (const o of otos) {
          const [from, to] = usableSpan(o, durationMs);
          ctx.fillStyle = boundary;
          // 切り出しの両端。**太い線。**
          ctx.fillRect(at(from), 0, Math.max(dpr, dpr * 1.5), h);
          ctx.fillRect(at(to) - dpr, 0, Math.max(dpr, dpr * 1.5), h);
          // 子音の終わり＝母音の始まり。**破線にして端と区別する。**
          const c = at(o.offset_ms + o.consonant_ms);
          for (let y = 0; y < h; y += dpr * 6) {
            ctx.fillRect(c, y, Math.max(1, dpr), dpr * 3);
          }
          // 先行発声。**下半分だけの短い線。**
          ctx.fillRect(at(o.offset_ms + o.preutterance_ms), h * 0.6, Math.max(1, dpr), h * 0.4);
          ctx.fillText(o.alias, at(from) + dpr * 3, dpr * 2);
        }
      })
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [takeId, span, peak, otos, durationMs]);

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

      {/*
        **見なくても判断できる代替を持たせる**（TR-PLT-32）。
        canvas は読み上げに何も出さないので、同じことを表で出す。
        **絵の説明ではなく、同じ判断ができる中身にする。**
      */}
      {otos.length > 0 && (
        <table className="w-full font-mono text-xs tabular-nums">
          <caption className="pb-1 text-left font-sans text-sm text-text-dim">
            自動で決めた切り出し（TR-ALN-33）
          </caption>
          <thead className="text-text-dim">
            <tr>
              <th scope="col" className="text-left font-normal">
                読み
              </th>
              <th scope="col" className="text-right font-normal">
                始まり
              </th>
              <th scope="col" className="text-right font-normal">
                終わり
              </th>
              <th scope="col" className="text-right font-normal">
                長さ
              </th>
              <th scope="col" className="text-right font-normal">
                子音
              </th>
            </tr>
          </thead>
          <tbody>
            {otos.map((o) => {
              const [from, to] = usableSpan(o, durationMs);
              return (
                <tr key={o.alias}>
                  <th scope="row" className="text-left font-sans font-normal">
                    {o.alias}
                  </th>
                  <td className="text-right">{from.toFixed(0)} ms</td>
                  <td className="text-right">{to.toFixed(0)} ms</td>
                  <td className="text-right">{(to - from).toFixed(0)} ms</td>
                  <td className="text-right">{o.consonant_ms.toFixed(0)} ms</td>
                </tr>
              );
            })}
          </tbody>
        </table>
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
