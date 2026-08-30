-- テイクごとの計測値（TR-REC-16）と、取りこぼしの記録（TR-REC-07）。
--
-- **測るだけで、判定も指摘もしない。** 「小さすぎます」「歪んでいます」を出さない。
-- take_analysis（合成のための f0/amp/サムネイル）とは目的が違うので分ける。
CREATE TABLE take_metrics (
  take_id           INTEGER PRIMARY KEY NOT NULL REFERENCES takes(id) ON DELETE CASCADE,

  -- サンプルピーク（dBFS）。無音は -1000 で表す（SQLite に -inf は入らない）。
  peak_dbfs         REAL    NOT NULL,
  rms               REAL    NOT NULL,
  -- |x| >= 1.0 - 1LSB が3サンプル以上続いた回数。
  -- **書き出しの直前に一度だけ集計して提示する**（TR-REC-16）。
  full_scale_runs   INTEGER NOT NULL,
  dc_offset         REAL    NOT NULL,
  noise_floor_rms   REAL    NOT NULL,

  -- 無音マージン（TR-REC-38）。**足りなくてもトリミングしない。記録するだけ。**
  leading_margin_ms  REAL   NOT NULL,
  trailing_margin_ms REAL   NOT NULL,

  -- 取りこぼし（TR-REC-07）。**0 でなければテイクは自動的に無効になる。**
  discontinuities   INTEGER NOT NULL,
  -- プリロールから持ってきたフレーム数（TR-REC-19）。
  preroll_frames    INTEGER NOT NULL
) STRICT;
