import { useCallback, useEffect, useRef, useState } from "react";

import { Spinner } from "~/components/spinner";
import { Button } from "~/components/ui/button";
import { CLIP_THRESHOLD } from "~/lib/levels";
import { type OtoView, api, errorMessage } from "~/lib/ipc";

type TakeInspectorProps = {
  takeId: number;
  durationMs: number;
  /** 絶対値の最大。[`CLIP_THRESHOLD`] 以上なら割れたものとして扱う。 */
  peak: number;
};

/** スペクトログラムの高さ（周波数方向の段数）。 */
const SPECTRO_ROWS = 96;
/** 引いたときの1画面の最短（ミリ秒）。これ以上は寄れない。 */
const MIN_SPAN_MS = 20;

/**
 * 切り出して使う区間（ミリ秒）。
 *
 * cutoff は負なら「offset からの長さ」、正なら「ファイル末尾からの距離」。
 * UTAU の慣例で、符号で意味が変わる。
 */
/**
 * CSS のカスタムプロパティを RGB の3値にする。
 *
 * Radix は16進で持っているので、そこから読む。
 * 読めなければ黒に倒す——描かないより、暗く出たほうが気づける。
 */
const readRgb = (styles: CSSStyleDeclaration, name: string): [number, number, number] => {
  const hex = styles.getPropertyValue(name).trim();
  const m = /^#([0-9a-f]{6})$/i.exec(hex);
  if (m?.[1] === undefined) return [0, 0, 0];
  const n = Number.parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

const usableSpan = (o: OtoView, fileMs: number): [number, number] => {
  const usable = o.cutoff_ms <= 0 ? -o.cutoff_ms : Math.max(0, fileMs - o.offset_ms - o.cutoff_ms);
  return [o.offset_ms, o.offset_ms + usable];
};

/**
 * 録れたテイクの波形とスペクトログラム（`TR-PLT-04`）。
 *
 * アプリが所有する単一の描画面へ直接描く。
 * 標準コントロールを並べて作らない。
 *
 * 可視域のみ計算し、可視域のみ描く。 ズーム倍率が変わっても、
 * 読む量は表示中の画素数に比例する——範囲の広さには比例しない。
 * Rust 側が段を積んでいるので、引いても寄っても同じ量しか渡ってこない。
 *
 * 素材全体の STFT を先に計算しない。 スペクトログラムも見えている範囲だけ。
 */
export const TakeInspector = ({ takeId, durationMs, peak }: TakeInspectorProps) => {
  const waveRef = useRef<HTMLCanvasElement>(null);
  const spectroRef = useRef<HTMLCanvasElement>(null);
  /** いま有効な描画か。外れたら、届いた応答を捨てる。 */
  const alive = useRef(true);
  const aliveSpectro = useRef(true);
  /**
   * いま見ている時間の範囲。
   *
   * テイクが変わったら全体へ戻る。effect で戻さず、呼び出し側が `key` を変える。
   * props から state を書き戻すと、戻す前の1フレームが古い値で描かれる。
   */
  const [span, setSpan] = useState<[number, number]>([0, durationMs]);
  const [showSpectro, setShowSpectro] = useState(false);
  const [otos, setOtos] = useState<OtoView[]>([]);
  const [error, setError] = useState<string | null>(null);
  /**
   * 描く元を取っている最中か。
   *
   * `waveform_window` と `spectrogram_window` は WAV を読んで畳むので、
   * 範囲を変えるたびに待つ。 待っている間 canvas は前の絵のままなので、
   * 何も出さないと「変わっていない」と読める。
   */
  const [drawing, setDrawing] = useState(false);

  // 自動原音設定が指した位置（`TR-ALN-33`）。テイクが変わったら引き直す。
  useEffect(() => {
    api
      .otosOfTake(takeId)
      .then(setOtos)
      .catch(() => setOtos([]));
  }, [takeId]);

  const draw = useCallback(() => {
    const canvas = waveRef.current;
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
    const clipped = peak >= CLIP_THRESHOLD;
    const wave = styles.getPropertyValue(clipped ? "--red-11" : "--cyan-11").trim();

    const boundary = styles.getPropertyValue("--amber-11").trim();
    const band = styles.getPropertyValue("--amber-3").trim();
    /** ミリ秒を画素へ。 */
    const at = (ms: number) => ((ms - span[0]) / (span[1] - span[0])) * w;

    setDrawing(true);
    api
      .waveformWindow({ takeId, fromMs: span[0], toMs: span[1], pixels: w })
      .then((points) => {
        if (!alive.current) return;
        ctx.clearRect(0, 0, w, h);

        /*
         * 切り出して使う区間を1回だけ求める。
         *
         * 下地（1）と境界（3）で同じ区間を使う。 それぞれで計算すると、
         * エイリアスの数だけ同じ計算を二度する。
         */
        const spans = otos.map((o) => ({ oto: o, span: usableSpan(o, durationMs) }));

        // ── 1. 切り出して使う区間を下地に敷く（`TR-ALN-33`）──
        //
        // 波形より先に描く。 上に乗せると波形が隠れる。
        ctx.fillStyle = band;
        for (const {
          span: [from, to],
        } of spans) {
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

        // ── 3. 境界と、その意味（`TR-ALN-33`）──
        //
        // 数字だけでは、発声と重なっているかが分からない。
        // 4モーラが 100ms に潰れていても「確信度 30%」としか出なかった。
        ctx.font = `${Math.round(11 * dpr)}px ui-monospace, monospace`;
        ctx.textBaseline = "top";
        for (const {
          oto: o,
          span: [from, to],
        } of spans) {
          ctx.fillStyle = boundary;
          // 切り出しの両端。太い線。
          ctx.fillRect(at(from), 0, Math.max(dpr, dpr * 1.5), h);
          ctx.fillRect(at(to) - dpr, 0, Math.max(dpr, dpr * 1.5), h);
          // 子音の終わり＝母音の始まり。破線にして端と区別する。
          const c = at(o.offset_ms + o.consonant_ms);
          for (let y = 0; y < h; y += dpr * 6) {
            ctx.fillRect(c, y, Math.max(1, dpr), dpr * 3);
          }
          // 先行発声。下半分だけの短い線。
          ctx.fillRect(at(o.offset_ms + o.preutterance_ms), h * 0.6, Math.max(1, dpr), h * 0.4);
          ctx.fillText(o.alias, at(from) + dpr * 3, dpr * 2);
        }
      })
      .catch((e: unknown) => setError(errorMessage(e)))
      // 失敗しても下ろす。下ろさないと、印が出たまま止まる。
      .finally(() => {
        if (alive.current) setDrawing(false);
      });
  }, [takeId, span, peak, otos, durationMs]);

  const drawSpectro = useCallback(() => {
    const canvas = spectroRef.current;
    if (canvas === null || !showSpectro) return;
    const ctx = canvas.getContext("2d");
    if (ctx === null) return;

    const styles = getComputedStyle(document.documentElement);
    const rect = canvas.getBoundingClientRect();
    // 列は画素より粗くてよい。 引き伸ばして描く。
    const columns = Math.max(1, Math.min(256, Math.round(rect.width / 3)));
    canvas.width = columns;
    canvas.height = SPECTRO_ROWS;

    // 濃淡の両端をトークンから取る。直接 RGB を書くと明暗に追従せず、
    // `check-contrast.ts` の網羅検査にも引っかからない。
    const lo = readRgb(styles, "--slate-3");
    const hi = readRgb(styles, "--cyan-11");

    api
      .spectrogramWindow({ takeId, fromMs: span[0], toMs: span[1], columns, rows: SPECTRO_ROWS })
      .then((s) => {
        if (!aliveSpectro.current) return;
        const image = ctx.createImageData(s.columns, s.rows);

        /*
         * ループの外へ出せるものを全部出す。
         *
         * 256×96 で 24,576 回まわる。 中で `lo[0]` や `hi[0] - lo[0]` を
         * 読み直すと、同じ添字参照と同じ引き算をその回数だけ繰り返す。
         * 両端の色は1回の描画で変わらないので、差分まで先に畳んでおく。
         */
        const [lo0, lo1, lo2] = lo;
        const d0 = hi[0] - lo0;
        const d1 = hi[1] - lo1;
        const d2 = hi[2] - lo2;
        const { columns, rows, bins } = s;
        const data = image.data;

        for (let c = 0; c < columns; c += 1) {
          for (let r = 0; r < rows; r += 1) {
            // 下が低い周波数になるよう、上下を返す。
            const v = bins[c * rows + (rows - 1 - r)] ?? 0;
            const at = (r * columns + c) * 4;
            // 面の色から波形の色へ寄せる。段の意味に沿うので明暗どちらでも読める。
            const t = v / 255;
            data[at] = Math.round(lo0 + d0 * t);
            data[at + 1] = Math.round(lo1 + d1 * t);
            data[at + 2] = Math.round(lo2 + d2 * t);
            data[at + 3] = 255;
          }
        }
        ctx.putImageData(image, 0, 0);
      })
      .catch((e: unknown) => setError(errorMessage(e)));
  }, [takeId, span, showSpectro]);

  // 古い応答で描かない（`DEC-PLT-017`）。`invoke` は応答の順序を保証しないので、
  // 拡大縮小を続けて押すと、前の範囲の応答が後から届いて古い時間窓を描く。
  useEffect(() => {
    alive.current = true;
    draw();
    return () => {
      alive.current = false;
    };
  }, [draw]);

  useEffect(() => {
    aliveSpectro.current = true;
    drawSpectro();
    return () => {
      aliveSpectro.current = false;
    };
  }, [drawSpectro]);

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
    <div className="flex flex-col gap-2">
      {/*
        待っている印を canvas の上に重ねる。 canvas は前の絵のままなので、
        何も出さないと「変わっていない」と読める。
        入れ替えない——差し替えると絵が消えて、待つたびに画面が空白になる。
      */}
      <div className="relative">
        <canvas
          ref={waveRef}
          role="img"
          aria-label={label}
          className="h-24 w-full rounded-lg bg-slate-3"
        />
        {drawing ? (
          <span className="absolute inset-0 flex items-center justify-center rounded-lg bg-slate-1/60">
            <Spinner />
          </span>
        ) : null}
      </div>

      {showSpectro && (
        <canvas
          ref={spectroRef}
          role="img"
          aria-label={`スペクトログラム。${(span[0] / 1000).toFixed(2)} 秒から ${(span[1] / 1000).toFixed(2)} 秒`}
          className="h-32 w-full rounded-lg bg-slate-3"
          style={{ imageRendering: "pixelated" }}
        />
      )}

      {/*
        見なくても判断できる代替を持たせる（TR-PLT-32）。
        canvas は読み上げに何も出さないので、同じことを表で出す。
        絵の説明ではなく、同じ判断ができる中身にする。
      */}
      {otos.length > 0 && (
        <table className="w-full select-text font-mono text-xs tabular-nums">
          <caption className="pb-1 text-left font-sans text-sm text-slate-11">
            自動で決めた切り出し（TR-ALN-33）
          </caption>
          <thead className="text-slate-11">
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
        <span className="font-mono text-xs text-slate-11 tabular-nums">
          {(span[0] / 1000).toFixed(2)} – {(span[1] / 1000).toFixed(2)} 秒
        </span>
      </div>

      {error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {error}
        </p>
      )}
    </div>
  );
};
