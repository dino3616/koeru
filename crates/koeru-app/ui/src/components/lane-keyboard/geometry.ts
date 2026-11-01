import { isBlackKey, whiteKeysBelow } from "~/lib/tones";

/** 鍵盤の範囲か窓。 両端を含む MIDI 番号。 */
export type ToneSpan = readonly [from: number, to: number];

/**
 * 黒鍵の幅（白鍵に対する比）。 描くのは `LaneKeyboard` の `w-7/10`。
 * 変えるときは両方を揃える。
 */
const BLACK_KEY_WIDTH = 0.7;

/**
 * 窓の両端を白鍵へ揃える。
 *
 * 黒鍵で切ると、端の鍵が半分だけ描かれる。 半分の鍵は押せる的として小さく
 * （`TR-PLT-31`）、どの音階なのかも読み取れない。 黒鍵が端なら、外側の白鍵まで広げる。
 */
export const snapSpan = ([from, to]: ToneSpan): ToneSpan => [
  isBlackKey(from) ? from - 1 : from,
  isBlackKey(to) ? to + 1 : to,
];

/**
 * 窓の中での鍵の位置。 窓の幅を 1 としたときの左端と幅。
 *
 * 担当音域の帯（`#438`）のように、鍵盤の上に重ねるものが同じ座標で描くために出す。
 * 窓の外の鍵は 0 未満や 1 超を返す。
 */
export const keySpan = (midi: number, view: ToneSpan): { left: number; width: number } => {
  const [from, to] = snapSpan(view);
  const origin = whiteKeysBelow(from);
  const total = whiteKeysBelow(to) + 1 - origin;
  const below = whiteKeysBelow(midi);
  if (isBlackKey(midi)) {
    return { left: (below - BLACK_KEY_WIDTH / 2 - origin) / total, width: BLACK_KEY_WIDTH / total };
  }
  return { left: (below - origin) / total, width: 1 / total };
};
