import { tv } from "~/lib/tv";

/** 拍の点。点の数と、いまの拍（まだ始まっていなければ `null`）。 */
type Beats = { count: number; current: number | null };

type ScriptRowProps = {
  /** 行のモーラ。 「きゃ」のような2字の拍は1要素で持つ。 1つ以上。 */
  moras: readonly string[];
  /**
   * いま歌うモーラの位置（0 始まり）。
   * 範囲の外は「いまのモーラが無い」で、負なら全部が未、長さ以上なら全部が済み。
   */
  position: number;
  /** ガイド BGM の拍。 `null` なら点を出さない。 進めるのは置く側で、ここは時計を持たない。 */
  beats: Beats | null;
};

/*
 * 色は声の色・字の2段だけ。 字の3段目は置かない（`DEC-PLT-062`）。
 * 声の色は置く側の要素が `voiceStyle` で与える。 声が無い音源では変数が無く、
 * `text-voice` は継承した色に落ちる——灰色の声を作らない。
 */
const mora = tv({
  variants: {
    state: {
      done: "text-1",
      current: "text-voice",
      upcoming: "text-2",
    },
  },
});

const dot = tv({
  variants: {
    state: {
      done: "bg-voice",
      current: "bg-slate-12",
      upcoming: "bg-slate-7",
    },
  },
});

const stateAt = (index: number, current: number) =>
  index < current ? "done" : index === current ? "current" : "upcoming";

/**
 * 録音リストの行（`P6` の「録音リストの行と入力レベル」）。
 *
 * 行のモーラを大きな字で並べ、いま歌うものだけを声の色にする。
 * 下の拍の点はガイド BGM の拍で、モーラの数とは別に数を受ける。
 *
 * 折れるのはモーラの境目だけ。 1拍ずつ flex の子にして `whitespace-nowrap` を掛けるので、
 * 「きゃ」が「き / ゃ」に割れない。 日本語は既定だと字数で折られる。
 *
 * 読み上げは行の全体を1回だけ。 いまの位置も拍も名前に入れない——入れると
 * 拍のたびに名前が書き換わり、`aria-live` を付けなくても読み直される。
 * 中身は `role="img"` の子として潰れ、見た目だけの要素は `aria-hidden` にしてある。
 * 字そのものは、読み上げるのが人なので選択できるまま残す。
 */
export const ScriptRow = ({ moras, position, beats }: ScriptRowProps) => (
  <div role="img" aria-label={moras.join(" ")} className="flex flex-col items-center gap-4">
    <p
      aria-hidden="true"
      className="flex flex-wrap items-center justify-center gap-x-5 text-7xl font-black select-text"
    >
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

    {beats !== null && (
      <p aria-hidden="true" className="flex items-center justify-center gap-10">
        {Array.from({ length: beats.count }, (_, i) => {
          const state = beats.current === null ? "upcoming" : stateAt(i, beats.current);
          return (
            // 点の大きさが変わっても行が揺れないよう、枠は一定にして中を拡げる。
            // 点は入れ替わらないので、位置が名前になる。
            <span key={i} className="flex size-2.5 items-center justify-center">
              <span
                data-state={state}
                className={`rounded-full ${state === "current" ? "size-2.5" : "size-2"} ${dot({ state })}`}
              />
            </span>
          );
        })}
      </p>
    )}
  </div>
);
