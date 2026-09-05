-- 録音停止時に算出する解析値（TR-PKG-05, TR-PKG-42）。
--
-- 書き出しと再開で WAV を再走査しないために置く。
-- 3時間ぶんの WAV を起動のたびに読み直したら、再開が即座でなくなる。
--
-- f0 / amp / thumbnail は f64 と u8 の並びを little-endian でそのまま持つ。
-- 正規化して行に割らない。 1テイクあたり数千フレームあり、行にすると
-- テイク数×フレーム数の行が生まれる。ここは常に一括で読み書きする塊。
CREATE TABLE take_analysis (
  take_id   INTEGER PRIMARY KEY NOT NULL REFERENCES takes(id) ON DELETE CASCADE,

  -- 波形のピーク（0.0〜1.0）。クリップ判定と表示に使う。
  peak      REAL    NOT NULL,

  -- .frq の hop（サンプル）。書式上は 256 固定だが、値として持つ。
  hop_size  INTEGER NOT NULL,

  -- フレームごとの F0（Hz、f64 LE の並び）。無声は 0。
  f0        BLOB    NOT NULL,
  -- フレームごとの振幅（f64 LE の並び）。
  amp       BLOB    NOT NULL,

  -- 波形サムネイル（u8 の並び）。バケットごとのピークを 0〜255 で持つ。
  thumbnail BLOB    NOT NULL
) STRICT;
