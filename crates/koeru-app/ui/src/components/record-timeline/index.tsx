import type { CSSProperties, ReactNode } from "react";

import type { PitchTrack } from "~/lib/ipc";
import { toneName } from "~/lib/tones";
import { tv } from "~/lib/tv";

/** 波形の欄の棒の数。 尺の長さによらず同じ本数を並べる（S04）。 */
export const WAVE_BARS = 300;

/** マスターのレート（`TR-REC-02`）。 `PitchTrack.hop` はこの標本数で数える。 */
const MASTER_RATE = 44_100;

/**
 * 尺の長さ（ミリ秒）。 拍の帯のマスの数 × 拍の長さ。
 *
 * マスは、助走1・モーラ・末尾1。 テイクがこれより長ければ、末尾のマスを足して覆う。
 * 尺の 0 は録りはじめる1拍前（過ぎたカウントの最後の拍）で、テイクの先頭は2マス目の頭。
 */
export const timelineCells = (moras: number, beatMs: number, takeMs: number | null) =>
  Math.max(moras + 2, Math.ceil((beatMs + (takeMs ?? 0)) / beatMs));

type RecordTimelineProps = {
  /** 中央の行のモーラ。 拍の帯の字になる。 */
  moras: readonly string[];
  beatMs: number;
  /**
   * 尺に載せる音の長さ（テイクの先頭から）。 録っているあいだは経過。
   * 何も載せていなければ `null`。 これまでのマスが済みになる。
   */
  takeMs: number | null;
  /**
   * 波形の棒（`WAVE_BARS` 本）。 0〜1 の大きさ。 `null` はまだ音が無いところ。
   * 作るのは `useTimelineWave`。
   */
  wave: readonly (number | null)[];
  /** 波形の欄の名前。 何の音か（録音中・テイクの名前・いま入っている音）。 */
  waveLabel: string;
  /** 確定したテイクの F0。 読んでいる最中は `undefined`、解析がまだなら `null`。 */
  pitch: PitchTrack | null | undefined;
  /** 収録音階（MIDI）。 F0 の欄の中央。 */
  tone: number | null;
  /** 再生位置（テイクの先頭から）。 録っているあいだは `rec` の色で描く。 */
  playhead: { ms: number; recording: boolean } | null;
};

/*
 * 拍のマス。 助走は `line` の塗り、末尾は透明に `line` の線。 済みは `line-strong`、
 * まだは `surface-2`。 いまのマスは、進んだぶんだけ `line-strong` を内側に重ねる。
 */
const cell = tv({
  base: "relative flex h-6 flex-1 items-center justify-center overflow-hidden rounded-md",
  variants: {
    kind: {
      lead: "bg-line",
      done: "bg-line-strong",
      now: "bg-surface-2",
      upcoming: "bg-surface-2",
      tail: "border border-line",
    },
  },
});

/** 尺の上の位置（`left` の値）。 左右 15px を空けた幅に載せる。 */
const at = (fraction: number) => `calc(15px + (100% - 30px) * ${fraction})`;

/** Hz を MIDI の高さへ。 */
const midiOf = (hz: number) => 69 + 12 * Math.log2(hz / 440);

/**
 * F0 の欄の線。 縦は収録音階 ±1 半音を欄の上下（180px）に置く。 無声のところで切る
 * ——前後で埋めると、出していない高さを出したように見える。 欄の外へ出た声は欄が切る。
 */
const PitchLines = ({
  pitch,
  center,
  beatMs,
  spanMs,
}: {
  pitch: PitchTrack;
  center: number;
  beatMs: number;
  spanMs: number;
}) => {
  const runs: string[] = [];
  let run: string[] = [];
  pitch.hz.forEach((hz, i) => {
    if (hz === null || hz <= 0) {
      if (run.length > 0) runs.push(run.join(" "));
      run = [];
      return;
    }
    const ms = beatMs + (i * pitch.hop * 1000) / MASTER_RATE;
    const x = (ms / spanMs) * 1000;
    const y = 90 - (midiOf(hz) - center) * 90;
    run.push(`${x.toFixed(1)},${y.toFixed(1)}`);
  });
  if (run.length > 0) runs.push(run.join(" "));

  return (
    <svg
      viewBox="0 0 1000 180"
      preserveAspectRatio="none"
      aria-hidden="true"
      focusable="false"
      className="size-full text-1"
    >
      {runs.map((points, i) => (
        <polyline
          // 区間の並びは F0 から決まり、入れ替わらない。
          // oxlint-disable-next-line react/no-array-index-key
          key={i}
          points={points}
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
};

/** その時刻の F0。 無声なら `null`。 */
const pitchAt = (pitch: PitchTrack, ms: number) => {
  const i = Math.round((ms / 1000) * (MASTER_RATE / pitch.hop));
  const hz = pitch.hz[i];
  return hz === null || hz === undefined || hz <= 0 ? null : hz;
};

/** 声の出ているところの中ほどの高さ。 収録音階が分からないときの中央。 */
const middleOf = (pitch: PitchTrack) => {
  const voiced = pitch.hz.filter((h): h is number => h !== null && h > 0).map(midiOf);
  if (voiced.length === 0) return null;
  voiced.sort((a, b) => a - b);
  return Math.round(voiced[Math.floor(voiced.length / 2)] ?? 0);
};

/**
 * 収録の画面の中央の時間軸（S04）。 上から拍の帯、波形の欄、F0 の欄、時刻の欄。
 * 全部が同じ尺に載り、再生位置の線が縦に貫く。
 *
 * 値は props で受ける。 波形を受け続けるのは `useTimelineWave`。
 *
 * 録っているあいだの F0 は流さない（`DEC-PLT-036`）。 欄は目盛りだけを出し、
 * 確定してから線を描く。
 */
export const RecordTimeline = ({
  moras,
  beatMs,
  takeMs,
  wave,
  waveLabel,
  pitch,
  tone,
  playhead,
}: RecordTimelineProps) => {
  const cells = timelineCells(moras.length, beatMs, takeMs);
  const spanMs = cells * beatMs;
  const reached = takeMs === null ? 0 : beatMs + takeMs;
  const center = tone ?? (pitch === null || pitch === undefined ? null : middleOf(pitch));
  const head = playhead === null ? null : (beatMs + playhead.ms) / spanMs;
  const seconds = Math.floor(spanMs / 1000);
  const dotHz =
    playhead === null || playhead.recording || pitch === null || pitch === undefined
      ? null
      : pitchAt(pitch, playhead.ms);

  const pitchLabel =
    pitch === undefined
      ? "音高の記録"
      : pitch === null
        ? "音高の記録。 まだ解析しています"
        : center === null
          ? "音高の記録。 声の出ているところが見つかりませんでした"
          : `音高の記録。 中央は ${toneName(center)}`;

  const beats: ReactNode[] = Array.from({ length: cells }, (_, i) => {
    const mora = i >= 1 && i <= moras.length ? moras[i - 1] : undefined;
    const from = i * beatMs;
    const kind =
      i === 0
        ? "lead"
        : mora === undefined
          ? "tail"
          : reached >= from + beatMs
            ? "done"
            : reached > from
              ? "now"
              : "upcoming";
    return (
      <span
        // 拍の並びは位置そのもの。 入れ替わらない。
        // oxlint-disable-next-line react/no-array-index-key
        key={i}
        className={cell({ kind })}
      >
        {kind === "now" && (
          <span
            className="absolute inset-y-0 left-0 w-(--filled) bg-line-strong"
            // 進んだぶんは経過で変わる。 変わる値は変数で渡し、当てるのはクラス。
            style={{ "--filled": `${((reached - from) / beatMs) * 100}%` } as CSSProperties}
          />
        )}
        {mora !== undefined && <span className="relative text-xs text-2">{mora}</span>}
      </span>
    );
  });

  return (
    <div className="relative flex w-full flex-col gap-2">
      <div role="img" aria-label="ガイドBGM の拍" className="flex w-full gap-0.5 px-3.75">
        {beats}
      </div>

      <div className="overflow-hidden rounded-xl border border-line bg-surface-1">
        <div role="img" aria-label={waveLabel} className="flex h-30 items-center gap-px px-3.5">
          {wave.map((amp, i) => (
            <span
              // 棒は時刻の区画そのもの。 入れ替わらない。
              // oxlint-disable-next-line react/no-array-index-key
              key={i}
              className={
                amp === null
                  ? "h-0.5 flex-1 rounded-full bg-line"
                  : "h-(--h) flex-1 rounded-full bg-current text-1"
              }
              {...(amp === null
                ? {}
                : {
                    // 高さは音で変わる。 変わる値は変数で渡し、当てるのはクラス。
                    style: {
                      "--h": `${Math.max(2, Math.min(120, amp * 120))}px`,
                    } as CSSProperties,
                  })}
            />
          ))}
        </div>

        <div
          role="img"
          aria-label={pitchLabel}
          className="relative h-45.25 overflow-hidden border-t border-line"
        >
          <span aria-hidden="true" className="absolute inset-x-0 top-22.5 h-px bg-line-strong" />
          <span aria-hidden="true" className="absolute inset-x-0 top-11.25 h-px bg-surface-2" />
          <span aria-hidden="true" className="absolute inset-x-0 top-33.75 h-px bg-surface-2" />
          {center !== null && (
            <span aria-hidden="true" className="font-mono text-xs text-2">
              <span className="absolute top-2.5 left-2 -translate-y-1/2">
                {toneName(center + 1)}
              </span>
              <span title="収録音階" className="absolute top-22.5 left-2 -translate-y-1/2">
                {toneName(center)}
              </span>
              <span className="absolute top-42.5 left-2 -translate-y-1/2">
                {toneName(center - 1)}
              </span>
            </span>
          )}
          {pitch !== null && pitch !== undefined && center !== null && (
            <div className="absolute inset-x-3.75 inset-y-0">
              <PitchLines pitch={pitch} center={center} beatMs={beatMs} spanMs={spanMs} />
            </div>
          )}
          {head !== null && dotHz !== null && center !== null && (
            <span
              aria-hidden="true"
              className="absolute top-(--y) left-(--x) z-3 -mt-1.25 -ml-1.25 size-2.5 rounded-full bg-current text-1 ring-2 ring-slate-2"
              style={
                {
                  "--x": at(head),
                  "--y": `${90 - (midiOf(dotHz) - center) * 90}px`,
                } as CSSProperties
              }
            />
          )}
        </div>

        <div aria-hidden="true" className="relative h-6.25 border-t border-line">
          {Array.from({ length: seconds + 1 }, (_, s) => {
            const f = (s * 1000) / spanMs;
            const align =
              s === 0
                ? "-translate-x-0.5"
                : s === seconds
                  ? "ml-0.5 -translate-x-full"
                  : "-translate-x-1/2";
            return (
              <span key={s}>
                <span
                  className="absolute top-0 left-(--x) h-1 w-px bg-line-strong"
                  style={{ "--x": at(f) } as CSSProperties}
                />
                <span
                  className={`absolute top-1.5 left-(--x) font-mono text-xs whitespace-nowrap text-2 ${align}`}
                  style={{ "--x": at(f) } as CSSProperties}
                >
                  {s.toFixed(1)}
                  {s === seconds ? " s" : ""}
                </span>
              </span>
            );
          })}
        </div>
      </div>

      {head !== null && (
        <span
          aria-hidden="true"
          className={playheadClass(playhead?.recording ?? false)}
          style={{ "--x": at(head) } as CSSProperties}
        >
          <span className="absolute -top-1 -left-1 size-2.5 rounded-full bg-inherit" />
        </span>
      )}
    </div>
  );
};

/** 再生位置の線。 拍の帯の上端から時刻の欄の手前まで。 */
const playheadClass = (recording: boolean) =>
  `absolute top-0 bottom-6.5 left-(--x) z-2 -ml-px w-0.5 ${recording ? "bg-rec" : "bg-slate-12"}`;
