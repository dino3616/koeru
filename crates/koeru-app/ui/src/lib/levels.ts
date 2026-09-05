/**
 * 入力レベルの判定に使う閾値。
 *
 * 表示する側が複数あるので、判断はここ1箇所に置く。
 */

/**
 * クリップとみなす下限。
 *
 * 1.0 ではなく 0.999。理由と正本は Rust 側の
 * `koeru_core::analysis::CLIP_THRESHOLD`。ずれは
 * `koeru-app` の `画面のクリップ閾値がrustと一致する` が落とす。
 */
export const CLIP_THRESHOLD = 0.999;

/** これを下回ると小さすぎる。マイクの利得を上げてもらう。 */
export const TOO_QUIET = 0.02;
