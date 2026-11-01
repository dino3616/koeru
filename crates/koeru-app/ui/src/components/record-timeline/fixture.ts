import type { PitchTrack } from "~/lib/ipc";

/*
 * S04 のボードと同じ値の場面。 部品と画面の story が使い、ボードと重ねて撮って差を見る。
 */

/**
 * 波形の棒の高さ（px）。 ボード（S04-Unlocked）の 266 本。 S04-Record は先頭の 147 本。
 * 欄の高さ 120px で割ると大きさになる。
 */
const BOARD_BAR_PX = [
  2, 3, 3, 3, 3, 4, 4, 3, 3, 3, 2, 3, 3, 3, 4, 3, 3, 2, 4, 3, 4, 4, 2, 3, 4, 2, 4, 2, 4, 2, 3, 3, 3,
  4, 31, 55, 79, 73, 53, 53, 69, 70, 65, 73, 82, 70, 71, 69, 66, 65, 64, 66, 66, 48, 45, 33, 4, 3,
  3, 3, 3, 3, 4, 4, 30, 59, 77, 85, 59, 66, 68, 57, 56, 58, 79, 67, 58, 86, 88, 61, 56, 65, 55, 52,
  44, 29, 4, 2, 3, 3, 2, 3, 4, 4, 33, 53, 73, 85, 65, 65, 82, 66, 56, 66, 52, 69, 59, 77, 75, 75,
  68, 78, 75, 55, 39, 25, 4, 2, 4, 4, 3, 2, 4, 4, 35, 47, 63, 69, 63, 57, 73, 81, 72, 64, 69, 57,
  49, 60, 69, 69, 54, 69, 77, 69, 34, 25, 4, 32, 50, 82, 65, 46, 61, 63, 72, 74, 65, 89, 67, 63, 62,
  70, 69, 57, 64, 59, 56, 49, 25, 4, 3, 3, 3, 3, 3, 4, 4, 39, 53, 75, 90, 51, 75, 77, 60, 48, 56,
  71, 75, 53, 86, 90, 56, 64, 59, 64, 52, 52, 25, 4, 2, 3, 3, 2, 3, 4, 4, 27, 62, 82, 82, 67, 59,
  90, 59, 65, 58, 49, 75, 67, 81, 76, 80, 77, 83, 77, 55, 37, 21, 4, 2, 4, 4, 3, 2, 4, 4, 33, 40,
  72, 69, 70, 63, 74, 86, 72, 57, 63, 64, 53, 56, 70, 64, 60, 73, 69, 62, 42, 34, 4, 3, 3, 3, 4, 4,
  2,
];

/** 先頭の `recorded` 本だけ音がある、300 本の棒。 */
export const boardWave = (recorded: number): (number | null)[] =>
  Array.from({ length: 300 }, (_, i) => {
    const px = BOARD_BAR_PX[i];
    return i < recorded && px !== undefined ? px / 120 : null;
  });

/** モーラごとの落ち着いた高さ（欄の上からの px）。 ボードの polyline から読んだ。 */
const SETTLE_Y = [102.6, 97.2, 104.4, 99.0, 101.1, 100.2, 101.4, 100.5];

/**
 * ボードの F0 の線を作る値。 拍の 50〜450 ms で声が出て、下からしゃくり上げて落ち着き、
 * 200 ms から揺れる。 収録音階は A3（220 Hz）、欄の上下が ±1 半音。
 */
export const boardPitch = (moras: number, beatMs = 500): PitchTrack => {
  const hop = 256;
  const frameMs = (hop * 1000) / 44_100;
  const frames = Math.floor((moras * beatMs) / frameMs);
  return {
    hop,
    hz: Array.from({ length: frames }, (_, i) => {
      const t = i * frameMs;
      const m = Math.floor(t / beatMs);
      const local = t - m * beatMs;
      const settle = SETTLE_Y[m % SETTLE_Y.length] ?? 100;
      if (local < 50 || local >= 450) return null;
      const glide = (140 - settle) * Math.exp(-(local - 50) / 25);
      const wobble = local > 200 ? 3.5 * Math.sin((2 * Math.PI * (local - 200)) / 125) : 0;
      const y = settle + glide + wobble;
      return 220 * 2 ** ((90 - y) / 90 / 12);
    }),
  };
};
