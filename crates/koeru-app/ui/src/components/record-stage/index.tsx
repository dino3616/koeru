import { type ReactNode, useEffect, useRef, useState } from "react";

import { Breath } from "~/components/breath";
import { Button } from "~/components/button";
import { LiveWaveform } from "~/components/live-waveform";
import { RecordButton } from "~/components/record-button";
import { ScriptRow } from "~/components/script-row";
import { TakeWaveform } from "~/components/take-waveform";
import type { PitchTrack } from "~/lib/ipc";
import { toneName } from "~/lib/tones";

/** 収録の段。 `use-recorder` が持つ状態を、画面が1つに読み替えて渡す。 */
export type StagePhase = "idle" | "countdown" | "recording" | "settling";

type RecordStageProps = {
  /** 中央の行の字（空白で区切ったモーラ）。 録る行が無ければ `null`。 */
  rowText: string | null;
  phase: StagePhase;
  /** 助走の残りの拍。 `phase` が `countdown` のときだけ読む。 */
  count: number | null;
  /** 1拍の長さ（ミリ秒）。 録っているあいだ、いま歌うモーラをこれで進める。 */
  beatMs: number;
  /** 中央の行の採用テイク。 無ければ波形を出さない。 */
  take: { id: number; durationMs: number; peak: number } | null;
  /** そのテイクの F0。 読んでいる最中は `undefined`、解析がまだなら `null`。 */
  pitch: PitchTrack | null | undefined;
  /** マイクが選ばれているか。 選ばれていなければ録らせない。 */
  ready: boolean;
  /** 右上に置くテイクの切り替え。 */
  takes?: ReactNode;
  /** 一覧で1つ前の行があるか。 */
  canGoBack: boolean;
  onRecord: () => void;
  onStop: () => void;
  /** カウント中に押したとき。 まだ録りはじめていない。 */
  onCancel: () => void;
  onBack: () => void;
  onPlay: () => void;
};

/** マスターのレート（`TR-REC-02`）。 `PitchTrack.hop` はこの標本数で数える。 */
const MASTER_RATE = 44_100;

/** Hz を MIDI の高さへ。 名前を引くために使う。 */
const midiOf = (hz: number) => 69 + 12 * Math.log2(hz / 440);

/**
 * 確定したテイクの F0（`Take.pitch`）。 録りながらの F0 はまだ無い（`DEC-PLT-036`）。
 *
 * 縦は対数で、そのテイクの声の出ているところの上下に合わせる。 固定の音域に
 * 置くと、低い声の人ほど線が下端に潰れる。 無声のところは線を切る——前後で
 * 埋めると、出していない高さを出したように見える。
 */
const PitchTrace = ({ pitch, durationMs }: { pitch: PitchTrack; durationMs: number }) => {
  const ref = useRef<HTMLCanvasElement>(null);
  const voiced = pitch.hz.filter((h): h is number => h !== null && h > 0);
  const low = voiced.length === 0 ? null : Math.min(...voiced);
  const high = voiced.length === 0 ? null : Math.max(...voiced);

  useEffect(() => {
    const canvas = ref.current;
    const ctx = canvas?.getContext("2d");
    if (canvas === null || canvas === undefined || ctx === null || ctx === undefined) return;
    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    const w = Math.max(1, Math.round(rect.width * dpr));
    const h = Math.max(1, Math.round(rect.height * dpr));
    canvas.width = w;
    canvas.height = h;
    ctx.clearRect(0, 0, w, h);
    if (low === null || high === null) return;

    // 上下に半音ずつ余白を持たせる。 端の線が枠に貼り付かない。
    const top = Math.log2(high) + 1 / 12;
    const bottom = Math.log2(low) - 1 / 12;
    const y = (hz: number) => ((top - Math.log2(hz)) / (top - bottom)) * h;
    const x = (i: number) => ((i * pitch.hop * 1000) / MASTER_RATE / durationMs) * w;

    // 線の色は声の色。 canvas に `text-voice` を当て、計算済みの値を読む。
    ctx.strokeStyle = getComputedStyle(canvas).color;
    ctx.lineWidth = 2 * dpr;
    ctx.lineJoin = "round";
    ctx.beginPath();
    let drawing = false;
    pitch.hz.forEach((hz, i) => {
      if (hz === null || hz <= 0) {
        drawing = false;
        return;
      }
      if (drawing) ctx.lineTo(x(i), y(hz));
      else ctx.moveTo(x(i), y(hz));
      drawing = true;
    });
    ctx.stroke();
  }, [pitch, durationMs, low, high]);

  const label =
    low === null || high === null
      ? "音の高さの流れ。 声の出ているところが見つかりませんでした"
      : `音の高さの流れ。 低いところ ${toneName(Math.round(midiOf(low)))}、高いところ ${toneName(Math.round(midiOf(high)))}`;

  return (
    <canvas
      ref={ref}
      role="img"
      aria-label={label}
      className="h-full w-full rounded-lg border border-line bg-surface-1 text-voice"
    />
  );
};

/**
 * 録っているあいだの経過（ミリ秒）。 止まっていれば 0。
 *
 * 画面の時計で数える。 Rust から位置を引きに行かない——いま歌うモーラは
 * 目安で、拍に合わせて動けばよい。 描き直しは 50 ms ごと（`LiveWaveform` の通知と同じ）。
 */
const useElapsed = (running: boolean) => {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!running) return;
    const from = performance.now();
    const timer = window.setInterval(() => setElapsed(performance.now() - from), 50);
    return () => {
      window.clearInterval(timer);
      setElapsed(0);
    };
  }, [running]);
  return elapsed;
};

/**
 * 収録の画面の中央。 骨格は S04（読み上げ・拍・波形・F0・操作）。
 *
 * 値は props で受け、録る・止めるの判断は持たない。 状態機械は `use-recorder`。
 * 取りに行くのは波形を描く部品（`LiveWaveform`、`TakeWaveform`）だけ。
 *
 * 波形は、録っているあいだと、録ったテイクが無いあいだは「いま入っている音」。
 * 録る前からマイクが拾っているかを見られる（`TR-REC-43`）。 待機中で採用テイクが
 * あれば、その波形と F0 に替わる。
 *
 * いま歌うモーラは、録りはじめてから1拍ずつ進める（`TR-REC-23` の1モーラ＝1拍）。
 *
 * キーボードのショートカットを持たない。 録る・止める・前の行・再生はボタンだけで、
 * Tab で順に届く。
 */
export const RecordStage = ({
  rowText,
  phase,
  count,
  beatMs,
  take,
  pitch,
  ready,
  takes,
  canGoBack,
  onRecord,
  onStop,
  onCancel,
  onBack,
  onPlay,
}: RecordStageProps) => {
  const elapsed = useElapsed(phase === "recording");
  // 区切りは Rust の `RowTakesView.moras` と同じ（空白で割る）。
  const moras = rowText === null ? [] : rowText.split(/\s+/).filter((m) => m !== "");
  const position =
    phase === "recording" ? Math.floor(elapsed / beatMs) : phase === "settling" ? moras.length : -1;
  const showTake = phase === "idle" && take !== null;
  const moving = phase !== "idle";

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <div className="flex min-h-10 justify-end">{takes}</div>

      <div className="flex min-h-32 items-center justify-center">
        {moras.length > 0 ? (
          <ScriptRow
            moras={moras}
            position={position}
            beats={{
              count: moras.length,
              current: position >= 0 && position < moras.length ? position : null,
            }}
          />
        ) : (
          <p className="text-md text-2">この枠で録る行はもうありません。</p>
        )}
      </div>

      <div className="flex flex-col gap-3">
        {showTake ? (
          <TakeWaveform
            takeId={take.id}
            durationMs={take.durationMs}
            peak={take.peak}
            otos={[]}
            selected={null}
          />
        ) : (
          <LiveWaveform />
        )}
        <div className="h-45">
          {showTake && pitch !== undefined && pitch !== null ? (
            <PitchTrace pitch={pitch} durationMs={take.durationMs} />
          ) : (
            <div className="flex h-full items-center justify-center rounded-lg border border-line bg-surface-1 text-sm text-2">
              {showTake && pitch === undefined ? (
                <Breath size="sm" />
              ) : showTake ? (
                "音の高さはまだ解析しています。"
              ) : (
                "音の高さは、録り終えると出ます。"
              )}
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center justify-center gap-8">
        <Button variant="secondary" disabled={moving || !canGoBack} onClick={onBack}>
          前の行
        </Button>
        <RecordButton
          state={phase === "settling" ? "idle" : phase}
          {...(count === null ? {} : { count })}
          disabled={phase === "settling" || (phase === "idle" && (!ready || moras.length === 0))}
          onPress={phase === "idle" ? onRecord : phase === "countdown" ? onCancel : onStop}
        />
        <Button variant="secondary" disabled={moving || take === null} onClick={onPlay}>
          再生
        </Button>
      </div>
    </div>
  );
};
