import type { VoiceView } from "~/lib/ipc";
import { tv } from "~/lib/tv";
import { type ImageColor, voiceStyle } from "~/lib/voice-color";

/** 一辺（CSS ピクセル）。72 は身元、40 は棚、24 は行の中。 */
export type AvatarSize = 72 | 40 | 24;

type AvatarProps = {
  /** 音源名。顔の名前になるので、必ず渡す。 */
  name: string;
  size: AvatarSize;
  /**
   * 本人が置いた絵の URL。 あれば頭文字にも「新」にも勝つ。
   *
   * 取るのは画面の側。 ここで読みに行くと、サイドバーの棚に並ぶ音源の数だけ
   * この部品が問い合わせを持つことになる。
   */
  image?: string | null;
  /** 声から作った色。 1つも録れていなければ `null`。 */
  color: VoiceView | null;
  /** 本人が選んだイメージカラー。 `color` が無くても、これがあれば頭文字を塗る。 */
  imageColor?: ImageColor | null;
};

const avatar = tv({
  variants: {
    size: {
      "72": "size-18 text-3xl",
      "40": "size-10 text-xl",
      "24": "size-6 text-xs",
    },
  },
});

/**
 * 音源の顔。 サイドバーの棚、音源の面の身元、公開済みの面で使う（`DEC-PLT-062`）。
 *
 * 顔は3つで、上から順に優先する。
 *
 * 1. 絵。 中央を正方形に切り抜いて見せるだけ。 角丸も縁もかけない——絵には手を入れず、
 *    拡大縮小だけにする（`DEC-PLT-062`）。 書き出しの 100×100 の BMP へ変えるのは
 *    別の仕事で（`TR-PKG-07`）、ここで画素を作り直さない。
 * 2. 声の色の面に頭文字。 声が1つも録れていなくても、イメージカラーがあれば塗れる。
 * 3. 「新」。 色も絵も無いあいだ。 灰色の面に頭文字を置くと「灰色の声」になるので、
 *    塗らずに破線の輪郭にする。 色だけで伝えない（`TR-PLT-28`）ので、字も「新」にする。
 *
 * 角丸を掛けるのは KOERU が描く代替（2 と 3）だけで、一辺の 22%。 角丸の段は px で
 * 固定なので、3つの大きさで同じ比にするには割合で書くしかない
 * （`vite.config.ts` の `no-arbitrary-values` に理由つきで許している）。
 *
 * 名前は必ず言う。 絵は `alt` で、頭文字と「新」は `role="img"` の名前で。
 * 頭文字だけでは「ナ」が誰かが読み上げに届かない。 「新」の名前には状態を足す——
 * 見えている字が名前に含まれるようにして、見えているものと読まれるものを食い違わせない。
 */
export const Avatar = ({ name, size, image = null, color, imageColor = null }: AvatarProps) => {
  const label = name.trim() === "" ? "名前の無い音源" : name;
  const sizeClass = avatar({ size: String(size) as `${AvatarSize}` });

  if (image !== null) {
    return (
      <img
        src={image}
        alt={label}
        className={`shrink-0 bg-slate-3 object-cover select-none ${sizeClass}`}
      />
    );
  }

  const initial = initialOf(name);
  if (initial !== "" && (color !== null || imageColor !== null)) {
    return (
      <span
        role="img"
        aria-label={label}
        className={`flex shrink-0 items-center justify-center rounded-[22%] bg-voice font-black text-voice-ink select-none ${sizeClass}`}
        /*
         * 変数を差すだけで、見た目の値は持たない。 別ファイルの関数なので、
         * 規則からは中身が読めず動的な塊に見える（`voice-rings` と同じ）。
         */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle(color, imageColor)}
      >
        {initial}
      </span>
    );
  }

  return (
    <span
      role="img"
      aria-label={`${label}（新）`}
      className={`flex shrink-0 items-center justify-center rounded-[22%] border-2 border-dashed border-slate-7 bg-slate-2 font-black text-slate-11 select-none ${sizeClass}`}
    >
      新
    </span>
  );
};

/** 字を書記素に割る。 コードポイントや UTF-16 で割ると、絵文字や結合文字が壊れる。 */
const segmenter = new Intl.Segmenter("ja", { granularity: "grapheme" });

const firstGrapheme = (text: string): string =>
  segmenter.segment(text)[Symbol.iterator]().next().value?.segment ?? "";

/**
 * 音源名の最初の1文字。 英字なら大文字にする。
 *
 * 大文字にして1字でなくなるもの（`ß` は `SS`）は、そのまま返す。
 * 「1文字」が守れないほうが、大文字であることより悪い。
 */
const initialOf = (name: string): string => {
  const first = firstGrapheme(name.trim());
  const upper = first.toUpperCase();
  return firstGrapheme(upper) === upper ? upper : first;
};
