/*
 * 収録音高の既定（`TR-RCL-06`）。
 *
 * ここに置くのは「1本で始めるときの高さ」1個だけ。 推奨の並びは Rust が
 * `tone_suggestions` で返す（`TR-RCL-06` が正本）。**両方に並びを持たない**
 * ——片方だけ変わる。
 *
 * この1個だけを写しているのは、画面の初期状態が問い合わせの前に決まるから。
 * 待っているあいだ「音高が0本」の状態を作ると、そこで作れてしまう。
 */

/** 1本で始めるときの収録音高（A3、`koeru_core::preset::DEFAULT_TONE_MIDI`）。 */
export const DEFAULT_TONE_MIDI = 57;

/*
 * 音階の範囲と、音名・鍵の並び（`DEC-RCL-019`）。
 *
 * 音名の作り方の正本は Rust（`koeru_model::tone::name`）。 ここにあるのは、
 * 鍵盤が全部の鍵を描くのに、鍵ごとに問い合わせられないので写したもの。
 * 表記は同じ——C4 = 60、異名同音は `#` 側だけ。
 */

/** 音階として扱える最小の MIDI 番号（C-1）。 */
export const TONE_MIN_MIDI = 0;
/** 音階として扱える最大の MIDI 番号（G9）。 MIDI の標準と同じ範囲にしてある。 */
export const TONE_MAX_MIDI = 127;

const SEMITONES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;

/** 1オクターブの中で、その半音より下にある白鍵の数。 */
const WHITE_KEYS_BELOW = [0, 1, 1, 2, 2, 3, 4, 4, 5, 5, 6, 6] as const;

const pitchClass = (midi: number) => ((midi % 12) + 12) % 12;

/** 音名（`A3`、`F#4`）。 MIDI 番号は画面に出さない（`TR-REC-25`）。 */
export const toneName = (midi: number): string =>
  `${SEMITONES[pitchClass(midi)] ?? ""}${Math.floor(midi / 12) - 1}`;

/** 黒鍵か。 */
export const isBlackKey = (midi: number): boolean =>
  SEMITONES[pitchClass(midi)]?.includes("#") ?? false;

/** この音より下にある白鍵の数（0 番の C から数える）。 鍵の横位置を出すのに使う。 */
export const whiteKeysBelow = (midi: number): number =>
  Math.floor(midi / 12) * 7 + (WHITE_KEYS_BELOW[pitchClass(midi)] ?? 0);
