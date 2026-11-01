import {
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
  useId,
  useRef,
  useState,
} from "react";

import { isBlackKey, toneName } from "~/lib/tones";

import {
  BAR_WIDTH,
  KEYS_WIDTH,
  type PianoRollGeometry,
  type PitchRange,
  ROW_HEIGHT,
  RULER_HEIGHT,
  TICKS_PER_BEAT,
  pianoRollGeometry,
} from "./geometry";

export type PianoRollNote = {
  /** 曲の頭からの開始位置（ティック）。 */
  startTicks: number;
  /** 長さ（ティック）。 */
  ticks: number;
  /** 音高（MIDI 番号）。 */
  midi: number;
  /** 歌詞（1モーラぶん）。 */
  lyric: string;
  /** 歌わせるのに要るエイリアスが、まだ無い音符。 */
  missing: boolean;
};

type PianoRollProps = {
  /** 名前。 表の見出しと、横に送る領域の名前になる。 */
  label: string;
  notes: readonly PianoRollNote[];
  /** 1小節の長さ（ティック）。 4/4 なら 1920。 */
  ticksPerBar: number;
  /**
   * 縦に見せる音高。 渡さなければ、音符とマイクの印が収まる幅に少し余白を足す。
   *
   * 横は渡さない。 曲の長さが決め、窓は送って動かす。
   */
  pitchRange?: PitchRange;
  /** サブバンクの音階（MIDI 番号）。 録るための高さなので、鍵盤にマイクの印を付ける。 */
  micTones?: readonly number[];
  /**
   * 図の上に重ねるもの。 #456 の範囲のハンドルが、この座標で描く。
   *
   * 図と同じ SVG の中に入るので、座標は部品が持つ写像（`PianoRollGeometry`）で引く。
   */
  overlay?: (geometry: PianoRollGeometry) => ReactNode;
};

/** ミニマップの高さ。 */
const MINI_HEIGHT = 48;

/** 音符の最小の幅。 短い音符でも歌詞の字が読める。 */
const MIN_NOTE_WIDTH = 20;

/** 歌詞の字1つの幅。 `text-xs` が 12px。 */
const GLYPH_WIDTH = 12;

/** 音符と音高の端に足す余白（半音）。 */
const PITCH_MARGIN = 2;

/** 音符が1つも無いときの音高。 C4 のまわり。 */
const EMPTY_RANGE: PitchRange = { low: 55, high: 72 };

const defaultPitchRange = (
  notes: readonly PianoRollNote[],
  micTones: readonly number[],
): PitchRange => {
  const all = [...notes.map((n) => n.midi), ...micTones];
  if (all.length === 0) return EMPTY_RANGE;
  return {
    low: Math.max(0, Math.min(...all) - PITCH_MARGIN),
    high: Math.min(127, Math.max(...all) + PITCH_MARGIN),
  };
};

/** 「小節 3 の 2.5 拍目」の拍の部分。 1 始まりで、小数は2桁まで。 */
const beatInBar = (note: PianoRollNote, ticksPerBar: number) =>
  Number(((note.startTicks % ticksPerBar) / TICKS_PER_BEAT + 1).toFixed(2));

/**
 * 曲の音符を描くピアノロール。 左に鍵盤、上に小節の番号、上にミニマップ。
 *
 * 描くのは SVG で、`DEC-PLT-014` の Canvas 2D と矛盾しない。 あれが相手にしているのは
 * 波形とスペクトログラム（サンプル数が桁違い）で、音符は数百。 SVG なら
 * 強制カラーモードで形が残り、座標を重ねる側（#456）も DOM として扱える。
 * 音符が数千になって重くなったら、ここだけ Canvas に移す。
 *
 * **図そのものは支援技術から隠す。** 位置の情報は2次元で、読み上げに向かない。
 * 同じ内容を並べた表を、視覚的に隠して置く。 名前を付けた図に文字が載ると、
 * 歌詞が図の名前の下で二度読まれる。
 *
 * 足りないエイリアスは斜線と注意の色で言う。 色だけにしない。
 *
 * 見た目を変える注入口は持たない。 横幅は置く側の箱が決め、図は箱の中で送る。
 */
export const PianoRoll = ({
  label,
  notes,
  ticksPerBar,
  pitchRange,
  micTones = [],
  overlay,
}: PianoRollProps) => {
  const id = useId();
  const hatch = `${id}-hatch`;
  const beatGrid = `${id}-beat`;
  const barGrid = `${id}-bar`;

  const endTick = Math.max(0, ...notes.map((n) => n.startTicks + n.ticks));
  const range = pitchRange ?? defaultPitchRange(notes, micTones);
  const g = pianoRollGeometry({ ticksPerBar, endTick, pitchRange: range });
  const rows = Array.from({ length: range.high - range.low + 1 }, (_, i) => range.high - i);
  const mics = new Set(micTones);
  const drawn = notes.filter((n) => n.midi >= range.low && n.midi <= range.high);
  const missingCount = notes.filter((n) => n.missing).length;

  const scroller = useRef<HTMLDivElement | null>(null);
  // 鍵盤は横に送っても動かないので、窓の幅からは引く。
  const [view, setView] = useState({ left: 0, width: 0 });
  const windowOf = (el: HTMLElement) => ({
    left: el.scrollLeft,
    width: Math.max(0, el.clientWidth - KEYS_WIDTH),
  });

  /*
   * 窓の幅は ResizeObserver から受ける。 外の仕組みとの同期で、effect の本来の用途。
   * 観測を始めると最初の寸法がすぐ届くので、初期値を effect の中で入れない。
   */
  const attach = (el: HTMLDivElement | null) => {
    scroller.current = el;
    if (el === null) return;
    const observer = new ResizeObserver(() => setView(windowOf(el)));
    observer.observe(el);
    return () => observer.disconnect();
  };

  const sendTo = (left: number) => scroller.current?.scrollTo({ left });
  const firstBar = Math.min(g.bars, Math.floor(view.left / BAR_WIDTH) + 1);

  const jumpFromPointer = (e: PointerEvent<HTMLDivElement>) => {
    const box = e.currentTarget.getBoundingClientRect();
    const x = ((e.clientX - box.left) / box.width) * g.width;
    // 押したところが窓の中心に来るように送る。
    sendTo(x - view.width / 2);
  };

  const jumpFromKey = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = { ArrowLeft: -BAR_WIDTH, ArrowRight: BAR_WIDTH }[e.key];
    if (step !== undefined) sendTo(view.left + step);
    else if (e.key === "PageUp") sendTo(view.left - view.width);
    else if (e.key === "PageDown") sendTo(view.left + view.width);
    else if (e.key === "Home") sendTo(0);
    else if (e.key === "End") sendTo(g.width);
    else return;
    e.preventDefault();
  };

  const miniY = (midi: number) => ((range.high - midi) / rows.length) * MINI_HEIGHT;
  const miniH = Math.max(2, MINI_HEIGHT / rows.length);

  return (
    <div className="flex flex-col gap-2">
      {missingCount > 0 && (
        <p className="flex items-center gap-2 self-end text-xs text-slate-11">
          <svg aria-hidden="true" width="16" height="12" className="shrink-0">
            <defs>
              <pattern
                id={`${hatch}-legend`}
                width="6"
                height="6"
                patternUnits="userSpaceOnUse"
                patternTransform="rotate(45)"
              >
                <rect width="6" height="6" className="fill-slate-3" />
                <rect width="3" height="6" className="fill-attention" />
              </pattern>
            </defs>
            <rect
              x="0.5"
              y="0.5"
              width="15"
              height="11"
              rx="2"
              fill={`url(#${hatch}-legend)`}
              className="stroke-attention"
            />
          </svg>
          足りないエイリアス
          <span className="font-mono tabular-nums">{missingCount}</span>
        </p>
      )}

      {/* ミニマップ。 窓の枠が、いま見えているところ。 */}
      <div
        role="slider"
        tabIndex={0}
        aria-label="見ている位置"
        aria-orientation="horizontal"
        aria-valuemin={1}
        aria-valuemax={g.bars}
        aria-valuenow={firstBar}
        aria-valuetext={`小節 ${firstBar}（全 ${g.bars} 小節）`}
        onKeyDown={jumpFromKey}
        onPointerDown={jumpFromPointer}
        // 押したまま動かすと追う。 枠の外へ出たら止まる。
        onPointerMove={(e) => {
          if (e.buttons === 1) jumpFromPointer(e);
        }}
        className="block cursor-pointer touch-none overflow-hidden rounded-lg border border-line bg-surface-1"
      >
        <svg
          aria-hidden="true"
          viewBox={`0 0 ${g.width} ${MINI_HEIGHT}`}
          preserveAspectRatio="none"
          className="block h-12 w-full"
        >
          {notes.map((n) => (
            <rect
              key={`${n.startTicks}-${n.midi}`}
              x={g.tickToX(n.startTicks)}
              y={miniY(n.midi)}
              width={Math.max(2, g.tickToX(n.ticks))}
              height={miniH}
              className={n.missing ? "fill-attention" : "fill-slate-11"}
            />
          ))}
          <rect
            x={view.left}
            y="1"
            width={Math.min(view.width, g.width)}
            height={MINI_HEIGHT - 2}
            strokeWidth="2"
            vectorEffect="non-scaling-stroke"
            className="fill-transparent stroke-slate-12"
          />
        </svg>
      </div>

      {/* 図。 矢印キーで横に送れるよう、送る領域そのものに焦点を置く。 */}
      <div
        ref={attach}
        role="region"
        aria-label={`${label}の図。矢印キーで横に送る`}
        // 送れる領域には焦点が要る。 無いとキーボードでは横に送れない（axe の `scrollable-region-focusable`）。
        // oxlint-disable-next-line jsx-a11y/no-noninteractive-tabindex
        tabIndex={0}
        onScroll={(e) => setView(windowOf(e.currentTarget))}
        className="overflow-x-auto rounded-lg border border-line"
      >
        <div className="flex w-max">
          <svg
            aria-hidden="true"
            width={KEYS_WIDTH}
            height={g.height}
            className="sticky left-0 z-10 shrink-0"
          >
            <rect width={KEYS_WIDTH} height={RULER_HEIGHT} className="fill-slate-2" />
            {rows.map((midi) => {
              const y = g.midiToY(midi);
              const black = isBlackKey(midi);
              const mic = mics.has(midi);
              return (
                <g key={midi}>
                  <rect
                    y={y}
                    width={KEYS_WIDTH}
                    height={ROW_HEIGHT}
                    className="fill-slate-3 stroke-slate-6"
                  />
                  {black && (
                    <rect
                      y={y}
                      width={KEYS_WIDTH / 2}
                      height={ROW_HEIGHT}
                      className="fill-slate-5 stroke-slate-6"
                    />
                  )}
                  {mic && (
                    <g transform={`translate(6 ${y + ROW_HEIGHT / 2})`} className="fill-slate-12">
                      <rect x="1" y="-6" width="6" height="9" rx="3" />
                      <path
                        d="M-1 -1a5 5 0 0 0 10 0M4 4v3"
                        fill="none"
                        strokeWidth="1.5"
                        className="stroke-slate-12"
                      />
                    </g>
                  )}
                  {(mic || midi % 12 === 0) && (
                    <text
                      x={KEYS_WIDTH - 6}
                      y={y + ROW_HEIGHT / 2}
                      textAnchor="end"
                      dominantBaseline="central"
                      className="fill-slate-12 font-mono text-xs tabular-nums select-none"
                    >
                      {toneName(midi)}
                    </text>
                  )}
                </g>
              );
            })}
          </svg>

          <svg aria-hidden="true" width={g.width} height={g.height} className="shrink-0">
            <defs>
              {/* 拍と小節の線は、並べずに敷く。 小節が数百になっても要素が増えない。 */}
              <pattern
                id={beatGrid}
                width={BAR_WIDTH / (ticksPerBar / TICKS_PER_BEAT)}
                height={ROW_HEIGHT}
                patternUnits="userSpaceOnUse"
              >
                <line x1="0.5" x2="0.5" y1="0" y2={ROW_HEIGHT} className="stroke-slate-4" />
              </pattern>
              <pattern
                id={barGrid}
                width={BAR_WIDTH}
                height={ROW_HEIGHT}
                patternUnits="userSpaceOnUse"
              >
                <line x1="0.5" x2="0.5" y1="0" y2={ROW_HEIGHT} className="stroke-slate-7" />
              </pattern>
              <pattern
                id={hatch}
                width="6"
                height="6"
                patternUnits="userSpaceOnUse"
                patternTransform="rotate(45)"
              >
                <rect width="6" height="6" className="fill-slate-3" />
                <rect width="3" height="6" className="fill-attention" />
              </pattern>
            </defs>

            {rows.map((midi) => (
              <rect
                key={midi}
                y={g.midiToY(midi)}
                width={g.width}
                height={ROW_HEIGHT}
                className={
                  isBlackKey(midi) ? "fill-slate-2 stroke-slate-4" : "fill-slate-1 stroke-slate-4"
                }
              />
            ))}
            <rect
              y={RULER_HEIGHT}
              width={g.width}
              height={g.height - RULER_HEIGHT}
              fill={`url(#${beatGrid})`}
            />
            <rect width={g.width} height={RULER_HEIGHT} className="fill-slate-2" />
            <rect width={g.width} height={g.height} fill={`url(#${barGrid})`} />
            {Array.from({ length: g.bars }, (_, b) => (
              <text
                // 小節の同一性は先頭からの順番。
                // oxlint-disable-next-line react/no-array-index-key
                key={b}
                x={b * BAR_WIDTH + 6}
                y={RULER_HEIGHT / 2}
                dominantBaseline="central"
                className="fill-slate-11 font-mono text-xs tabular-nums select-none"
              >
                {b + 1}
              </text>
            ))}

            {drawn.map((n) => {
              const x = g.tickToX(n.startTicks) + 1;
              const y = g.midiToY(n.midi) + 2;
              const w = Math.max(MIN_NOTE_WIDTH, g.tickToX(n.ticks) - 2);
              const h = ROW_HEIGHT - 4;
              // 歌詞は、音符の頭に敷いた無地の面に載せる。 斜線の上に字を載せない。
              const plate = Math.min(w, n.lyric.length * GLYPH_WIDTH + 8);
              return (
                <g key={`${n.startTicks}-${n.midi}`}>
                  {n.missing ? (
                    <>
                      <rect
                        x={x}
                        y={y}
                        width={w}
                        height={h}
                        rx="3"
                        fill={`url(#${hatch})`}
                        className="stroke-attention"
                      />
                      <rect
                        x={x}
                        y={y}
                        width={plate}
                        height={h}
                        rx="3"
                        className="fill-slate-3 stroke-attention"
                      />
                    </>
                  ) : (
                    <rect x={x} y={y} width={w} height={h} rx="3" className="fill-slate-11" />
                  )}
                  <text
                    x={x + 4}
                    y={y + h / 2}
                    dominantBaseline="central"
                    className={
                      n.missing
                        ? "fill-slate-12 text-xs select-none"
                        : "fill-slate-1 text-xs select-none"
                    }
                  >
                    {n.lyric}
                  </text>
                </g>
              );
            })}

            {overlay?.(g)}
          </svg>
        </div>
      </div>

      <table className="sr-only">
        <caption>
          {label}の音符。
          {micTones.length > 0 && `録る音高は ${micTones.map(toneName).join("、")}。`}
        </caption>
        <thead>
          <tr>
            <th scope="col">小節</th>
            <th scope="col">拍</th>
            <th scope="col">歌詞</th>
            <th scope="col">音高</th>
            <th scope="col">長さ（拍）</th>
            <th scope="col">エイリアス</th>
          </tr>
        </thead>
        <tbody>
          {notes.map((n) => (
            <tr key={`${n.startTicks}-${n.midi}`}>
              <td>{Math.floor(n.startTicks / ticksPerBar) + 1}</td>
              <td>{beatInBar(n, ticksPerBar)}</td>
              <td>{n.lyric}</td>
              <td>{toneName(n.midi)}</td>
              <td>{Number((n.ticks / TICKS_PER_BEAT).toFixed(2))}</td>
              <td>{n.missing ? "足りない" : "ある"}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
};
