/*
 * 収録音高の既定（`TR-RCL-06`）。
 *
 * ここに置くのは「1本で始めるときの高さ」と、鍵盤が引く音名の写しだけ。
 * 推奨の並びは Rust が `tone_suggestions` で返す（`TR-RCL-06` が正本）。
 * **両方に並びを持たない**——片方だけ変わる。
 *
 * 既定を写しているのは、画面の初期状態が問い合わせの前に決まるから。
 * 待っているあいだ「音高が0本」の状態を作ると、そこで作れてしまう。
 */

/** 1本で始めるときの収録音高（A3、`koeru_core::preset::DEFAULT_TONE_MIDI`）。 */
export const DEFAULT_TONE_MIDI = 57;

/** 半音の名前。異名同音は `#` 側だけ（`koeru_model::tone::name` と同じ）。 */
const SEMITONES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"] as const;

/**
 * MIDI 番号の英語音名。 60 が `C4`（UTAU の表記。Yamaha 式の 60 = C3 ではない）。
 *
 * 鍵盤は 0〜127 の全部に、問い合わせを待たず名前を付けて描く。 そのための同期の写しで、
 * 名前の決め方の正本は Rust の `koeru_model::tone::name`。
 */
export const toneName = (midi: number): string =>
  `${SEMITONES[((midi % 12) + 12) % 12]}${Math.floor((midi - 60) / 12) + 4}`;

/** 黒鍵か。 */
export const isBlackKey = (midi: number): boolean =>
  [1, 3, 6, 8, 10].includes(((midi % 12) + 12) % 12);
