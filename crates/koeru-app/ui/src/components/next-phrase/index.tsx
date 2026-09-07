import { Breath } from "~/components/breath";
import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { Elapsed } from "~/components/elapsed";

type NextPhraseProps = {
  /** 次に読む行のテキスト。全部録れていれば `null`。 */
  text: string | null;
  /** その行から取れる音の数。 */
  units: number;
  recording: boolean;
  /** テイクを確かめている最中か。数秒かかる。 */
  settling: boolean;
  continuous: boolean;
  /** マイクを選べているか。選ぶまでは録らせない。 */
  ready: boolean;
  /** 連続収録の1フレーズの長さ（ミリ秒、`TR-REC-20`）。 */
  advanceMs: number;
  onStart: () => void;
  onStop: () => void;
  onContinuous: () => void;
  onPause: () => void;
};

/**
 * 次に読むところ（`DEC-PLT-024`、`TR-REC-18`）。
 *
 * 独立した面にしない。 収録項目のコレクションの上に作業帯として乗せ、
 * 次に読む1フレーズを大きく出す。
 *
 * 読み上げるフレーズだけ字間を空ける。 1音ずつ読ませるものなので、
 * 詰まっていると読み違える（`EVID-UX-003` に読み間違いの記録がある）。
 *
 * 内部表現を出さない（`TR-REC-18`）。 項目 ID もファイル名も出さない。
 *
 * 確かめている間は押せる的を出さない。 `finish_take` は解析と
 * アライメントを含むので数秒かかる。ここで「録る」を出すと、
 * 確定の途中で次を始めさせてしまう（`TR-REC-42`）。
 *
 * **「続けて録る」にも同じことが掛かる。** 差し替わるのは「録る」だけなので、
 * こちらは押せるまま残っていた。**踏んだ。** 最後の砦は `use-recorder` の
 * 札のほうで、ここは押せる的を出さないだけ。
 */
export const NextPhrase = ({
  text,
  units,
  recording,
  settling,
  continuous,
  ready,
  advanceMs,
  onStart,
  onStop,
  onContinuous,
  onPause,
}: NextPhraseProps) => (
  <Card title="次に読む">
    <p className="flex items-baseline justify-between gap-3">
      <span className="font-mono text-xs text-slate-11 tabular-nums">
        {text === null ? "" : `この行から ${units} 音`}
      </span>
    </p>

    <p className="select-text text-center text-4xl font-semibold leading-relaxed tracking-[0.18em] text-slate-12">
      {text ?? "全部読み終えました"}
    </p>

    {text !== null && (
      <p className="text-center text-xs text-slate-11">1 つずつ、間をあけて読みます。</p>
    )}

    <div className="flex flex-wrap items-center justify-center gap-2">
      {recording ? (
        <Button variant="danger" onClick={onStop}>
          {/*
            経過秒は名前に入れない。 入れるとフォーカス中の要素の
            accessible name が毎秒書き換わり、読み上げが追えなくなる。
          */}
          止める
          <Elapsed />
        </Button>
      ) : settling ? (
        <Button variant="primary" disabled>
          <Breath size="sm" />
          確かめています
        </Button>
      ) : (
        <Button
          variant="primary"
          onClick={onStart}
          disabled={!ready || text === null || continuous}
        >
          録る
        </Button>
      )}

      {/*
        連続収録（`TR-REC-20`）。1フレーズ固定長で進む。
        発話の検出結果を条件にしない。
      */}
      {continuous ? (
        <Button variant="secondary" onClick={onPause}>
          続けて録るのをやめる
        </Button>
      ) : (
        <Button onClick={onContinuous} disabled={!ready || text === null || recording || settling}>
          続けて録る
        </Button>
      )}
    </div>

    <p className="text-center text-xs text-slate-11">
      {!ready
        ? "設定でマイクを選ぶと録れます。"
        : continuous
          ? `1 フレーズ ${(advanceMs / 1000).toFixed(1)} 秒で次へ進みます。やめた行はまだ録っていないまま残ります。`
          : recording
            ? "言い終えたら「止める」を押します。押した 0.5 秒あとまで録ります。"
            : "「録る」は止めるまで録り続けます。押した 0.5 秒前から録れています。"}
    </p>
  </Card>
);
