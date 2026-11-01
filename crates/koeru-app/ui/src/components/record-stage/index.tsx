import { type ReactNode, useEffect, useId, useState } from "react";

import { Button } from "~/components/button";
import { Icon } from "~/components/icon";
import { RecordButton } from "~/components/record-button";
import { RecordTimeline, timelineCells } from "~/components/record-timeline";
import { type WaveSource, useTimelineWave } from "~/components/record-timeline/use-wave";
import { ScriptRow } from "~/components/script-row";
import type { PitchTrack } from "~/lib/ipc";

/** 収録の段。 `use-recorder` が持つ状態を、画面が1つに読み替えて渡す。 */
export type StagePhase = "idle" | "countdown" | "recording" | "settling";

type RecordStageProps = {
  /** 音源。 確定したテイクの波形を引く鍵に要る。 */
  voiceId: string;
  /** 中央の行の字（空白で区切ったモーラ）。 録る行が無ければ `null`。 */
  rowText: string | null;
  /** この行で作るエイリアス（作った順）。 */
  aliases: readonly string[];
  phase: StagePhase;
  /** 助走の残りの拍。 `phase` が `countdown` のときだけ読む。 */
  count: number | null;
  /** 1拍の長さ（ミリ秒）。 録っているあいだ、いま歌うモーラをこれで進める。 */
  beatMs: number;
  /** 中央の行の採用テイク。 無ければいま入っている音を出す。 */
  take: { id: number; durationMs: number; name: string } | null;
  /** そのテイクの F0。 読んでいる最中は `undefined`、解析がまだなら `null`。 */
  pitch: PitchTrack | null | undefined;
  /** 収録音階（MIDI）。 F0 の欄の中央。 */
  tone: number | null;
  /** マイクが選ばれているか。 選ばれていなければ録らせない。 */
  ready: boolean;
  /** 右上に置くテイクの切り替え。 */
  takes?: ReactNode;
  /** 操作の下に出す知らせ（失敗など）。 */
  notice?: ReactNode;
  onRecord: () => void;
  onStop: () => void;
  /** カウント中に押したとき。 まだ録りはじめていない。 */
  onCancel: () => void;
  /** 再生を押したとき。 鳴らすのは受け取った側。 */
  onPlay: () => void;
};

/** 描き直しの間隔。 包絡の通知と同じ 50 ms。 */
const TICK_MS = 50;

/**
 * 録っているあいだの経過（ミリ秒）。 止めても最後の値を持ち、次に録りはじめたら 0 から。
 *
 * 画面の時計で数える。 Rust から位置を引きに行かない——いま歌うモーラは
 * 目安で、拍に合わせて動けばよい。
 */
const useElapsed = (running: boolean) => {
  const [elapsed, setElapsed] = useState(0);
  useEffect(() => {
    if (!running) return;
    const from = performance.now();
    // 録りはじめは 0。 前のテイクの終わりを1拍ぶん見せない。
    // oxlint-disable-next-line react/set-state-in-effect
    setElapsed(0);
    const timer = window.setInterval(() => setElapsed(performance.now() - from), TICK_MS);
    return () => window.clearInterval(timer);
  }, [running]);
  return elapsed;
};

/**
 * 再生位置（テイクの先頭から）。 再生を押すと先頭から終わりまで画面の時計で進み、
 * 終わりで止まる。 先頭に戻すと 0。 テイクを替えると終わりに置き直す。
 *
 * 鳴っている位置を Rust から引かない。 再生は頭から最後まで流すだけで、途中から
 * 鳴らす口が無い。 線は「どこまで聴いたか」の目安。
 */
const usePlayhead = (take: { id: number; durationMs: number } | null) => {
  const [state, setState] = useState<{ takeId: number; ms: number; since: number | null }>({
    takeId: -1,
    ms: 0,
    since: null,
  });
  const mine = take !== null && state.takeId === take.id;
  const playing = mine && state.since !== null;
  const durationMs = take?.durationMs ?? 0;

  useEffect(() => {
    if (!playing) return;
    const timer = window.setInterval(() => {
      setState((s) => {
        if (s.since === null) return s;
        const ms = performance.now() - s.since;
        return ms >= durationMs ? { ...s, ms: durationMs, since: null } : { ...s, ms };
      });
    }, TICK_MS);
    return () => window.clearInterval(timer);
  }, [playing, durationMs]);

  const ms = take === null ? null : mine ? state.ms : take.durationMs;
  return {
    ms,
    play: () => {
      if (take !== null) setState({ takeId: take.id, ms: 0, since: performance.now() });
    },
    rewind: () => {
      if (take !== null) setState({ takeId: take.id, ms: 0, since: null });
    },
  };
};

/**
 * 収録の画面の中央（S04）。 右上にテイク、上からエイリアス・読み上げの行・時間軸・操作。
 *
 * 値は props で受け、録る・止めるの判断は持たない。 状態機械は `use-recorder`。
 * 取りに行くのは波形（`useTimelineWave`）だけ。
 *
 * 波形は、録っているあいだと、録ったテイクが無いあいだは「いま入っている音」。
 * 録る前からマイクが拾っているかを見られる（`TR-REC-43`、`DEC-REC-013`）。 待機中で
 * 採用テイクがあれば、その波形と F0 に替わる。
 *
 * いま歌うモーラは、録りはじめてから1拍ずつ進める（`TR-REC-23` の1モーラ＝1拍）。
 *
 * キーボードのショートカットを持たない。 先頭に戻す・録る・止める・再生はボタンだけで、
 * Tab で順に届く。 前の行へは左の列で移る。
 */
export const RecordStage = ({
  voiceId,
  rowText,
  aliases,
  phase,
  count,
  beatMs,
  take,
  pitch,
  tone,
  ready,
  takes,
  notice,
  onRecord,
  onStop,
  onCancel,
  onPlay,
}: RecordStageProps) => {
  const aliasesId = useId();
  const elapsed = useElapsed(phase === "recording");
  const shown = phase === "idle" ? take : null;
  const playhead = usePlayhead(shown);
  // 区切りは Rust の `RowTakesView.moras` と同じ（空白で割る）。
  const moras = rowText === null ? [] : rowText.split(/\s+/).filter((m) => m !== "");
  const position =
    phase === "recording" ? Math.floor(elapsed / beatMs) : phase === "settling" ? moras.length : -1;
  const moving = phase !== "idle";

  const takeMs =
    phase === "recording" || phase === "settling"
      ? elapsed
      : shown === null
        ? null
        : shown.durationMs;
  const spanMs = timelineCells(moras.length, beatMs, takeMs) * beatMs;
  const source: WaveSource =
    phase === "recording"
      ? { kind: "recording" }
      : phase === "settling"
        ? { kind: "hold" }
        : shown === null
          ? { kind: "live" }
          : { kind: "take", takeId: shown.id, durationMs: shown.durationMs };
  const wave = useTimelineWave({ voiceId, source, beatMs, spanMs });

  return (
    <section aria-label="この行" className="relative flex min-h-0 min-w-0 flex-1 overflow-y-auto">
      <div className="absolute top-5 right-10">{takes}</div>

      <div className="m-auto flex w-full flex-col items-center px-10 pt-9 pb-8">
        <div className="flex flex-col items-center gap-5">
          {aliases.length > 0 && (
            <div className="flex max-w-190 flex-col items-center">
              <div className="mb-2 flex items-baseline gap-2 text-xs text-2">
                <h2 id={aliasesId} className="font-semibold">
                  この行のエイリアス
                </h2>
                <span className="font-mono">{aliases.length}</span>
              </div>
              <ul aria-labelledby={aliasesId} className="flex flex-wrap justify-center gap-1">
                {aliases.map((alias, i) => (
                  <li
                    // 同じ綴りが2度出ることがある（重複は台帳が持つ）。 並びは作った順で動かない。
                    // oxlint-disable-next-line react/no-array-index-key
                    key={`${alias}-${i}`}
                    className="inline-flex h-6 items-center rounded-md bg-surface-2 px-2 font-mono text-xs whitespace-pre text-1"
                  >
                    {alias}
                  </li>
                ))}
              </ul>
            </div>
          )}
          {moras.length > 0 ? (
            <ScriptRow moras={moras} position={position} />
          ) : (
            <p className="text-md text-2">この枠で録る行はもうありません。</p>
          )}
        </div>

        <div className="mt-14 w-full max-w-258">
          <RecordTimeline
            moras={moras}
            beatMs={beatMs}
            takeMs={takeMs}
            wave={wave}
            waveLabel={
              phase === "recording"
                ? "入力の波形。録音中"
                : shown === null
                  ? "いま入っている音の形"
                  : `入力の波形。${shown.name}`
            }
            pitch={shown === null ? undefined : pitch}
            tone={tone}
            playhead={
              phase === "recording"
                ? { ms: elapsed, recording: true }
                : playhead.ms === null
                  ? null
                  : { ms: playhead.ms, recording: false }
            }
          />
        </div>

        <div role="group" aria-label="録音の操作" className="mt-8 flex items-center gap-5">
          <Button
            variant="raised"
            size="icon-lg"
            aria-label="再生ヘッドを先頭に戻す"
            title="先頭に戻す"
            disabled={moving || shown === null}
            onClick={playhead.rewind}
          >
            <Icon name="to-start" size={18} />
          </Button>
          <RecordButton
            state={phase === "settling" ? "idle" : phase}
            {...(count === null ? {} : { count })}
            disabled={phase === "settling" || (phase === "idle" && (!ready || moras.length === 0))}
            onPress={phase === "idle" ? onRecord : phase === "countdown" ? onCancel : onStop}
          />
          <Button
            variant="raised"
            size="icon-lg"
            aria-label="再生"
            disabled={moving || shown === null}
            onClick={() => {
              playhead.play();
              onPlay();
            }}
          >
            <Icon name="play" size={18} />
          </Button>
        </div>

        {notice}
      </div>
    </section>
  );
};
