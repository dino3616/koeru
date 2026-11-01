/**
 * バーの左端にあたる dBFS。 これより小さい音は左端に置く。
 *
 * 声の話し声がバーの左 1 割に潰れない幅として 30 dB を採った。 右端は 0 dBFS
 * （フルスケール）で動かさない——「いちばん右まで届くと、音が割れます」の右端。
 */
export const FLOOR_DBFS = -30;

/** 振幅（0.0〜1.0）を dBFS へ。 無音は `-Infinity`。 */
export const toDbfs = (amplitude: number): number =>
  amplitude <= 0 ? Number.NEGATIVE_INFINITY : 20 * Math.log10(amplitude);

/** バーの上の位置（0〜100 %）。 `FLOOR_DBFS` 以下は 0、フルスケール以上は 100。 */
export const toPercent = (amplitude: number): number => {
  const db = toDbfs(amplitude);
  return Math.min(100, Math.max(0, ((db - FLOOR_DBFS) / -FLOOR_DBFS) * 100));
};

/**
 * 数値の見せ方。 小数 1 桁で、負号は字幅のある `−`（U+2212）。
 *
 * `-0.0` を出さない。 丸めて 0 になるものは符号なしの `0.0`。
 */
export const formatDbfs = (amplitude: number): string => {
  const db = toDbfs(amplitude);
  if (db === Number.NEGATIVE_INFINITY) return "−∞";
  const rounded = Math.abs(db) < 0.05 ? 0 : db;
  return rounded.toFixed(1).replace("-", "−");
};
