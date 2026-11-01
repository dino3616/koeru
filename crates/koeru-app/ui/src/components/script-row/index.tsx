import { tv } from "~/lib/tv";

type ScriptRowProps = {
  /** 行のモーラ。 「きゃ」のような2字の拍は1要素で持つ。 1つ以上。 */
  moras: readonly string[];
  /**
   * いま歌うモーラの位置（0 始まり）。
   * 範囲の外は「いまのモーラが無い」で、負なら全部が未、長さ以上なら全部が済み。
   */
  position: number;
};

/**
 * 72px で1行に収まる字数。 中央の段の幅（1032px）を 72px で割った数。
 * これを超える行は、字を1段下げて（48px）描く。
 */
const LINE_CHARS = 14;

/*
 * 色は声の色・字の2段だけ。 字の3段目は置かない（`DEC-PLT-062`）。 済みは退き、
 * まだのものは薄くしない——これから歌う字がいちばん読める（S04）。
 *
 * 声の色は置く側の要素が `voiceStyle` で与える。 声が無い音源では変数が無く、
 * `text-voice` は継承した色に落ちる——灰色の声を作らない。
 */
const mora = tv({
  variants: {
    state: {
      done: "text-2",
      current: "text-voice",
      upcoming: "text-1",
    },
  },
});

const line = tv({
  base: "flex justify-center leading-none font-black select-text",
  variants: {
    fit: {
      true: "text-7xl whitespace-nowrap",
      false: "flex-wrap text-5xl",
    },
  },
});

const stateAt = (index: number, current: number) =>
  index < current ? "done" : index === current ? "current" : "upcoming";

/**
 * 録音リストの行（`P6` の「録音リストの行」）。
 *
 * 行のモーラを大きな字で詰めて並べ、いま歌うものだけを声の色にする。 拍は
 * 下の時間軸（`RecordTimeline`）が持つ。
 *
 * 折り返さない。 1行に収まらない字数の行は字を1段下げる。 それでも収まらない
 * 幅の窓では折るが、折れるのはモーラの境目だけ。 1拍ずつ flex の子にして
 * `whitespace-nowrap` を掛けるので、「きゃ」が「き / ゃ」に割れない。
 *
 * 読み上げは行の全体を1回だけ。 いまの位置を名前に入れない——入れると
 * 拍のたびに名前が書き換わり、`aria-live` を付けなくても読み直される。
 * 中身は `role="img"` の子として潰れ、見た目だけの要素は `aria-hidden` にしてある。
 * 字そのものは、読み上げるのが人なので選択できるまま残す。
 */
export const ScriptRow = ({ moras, position }: ScriptRowProps) => (
  <div role="img" aria-label={moras.join(" ")}>
    <p aria-hidden="true" className={line({ fit: moras.join("").length <= LINE_CHARS })}>
      {moras.map((m, i) => {
        const state = stateAt(i, position);
        return (
          <span
            // 同じ拍が1行に2度出る（「か か き」）。 位置でしか区別できない。
            key={`${m}-${i}`}
            data-state={state}
            className={`whitespace-nowrap ${mora({ state })}`}
          >
            {m}
          </span>
        );
      })}
    </p>
  </div>
);
