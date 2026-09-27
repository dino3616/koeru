-- 欠落の位置（TR-REC-07 の「欠落の発生数と位置をメタデータに記録する」）。
--
-- 位置はテイクの先頭（Pump::start_take の `from`）からの、マスター（44100 Hz）の
-- 標本位置。 種類は取りこぼし（dropped）・不連続（discontinuity）・レンダの失敗
-- （render_error）のいずれか。 固定長の領域（koeru-audio の GAP_LOG_CAPACITY 件）に
-- 収まらなかったぶんは take_metrics.gaps_overflowed の件数だけが増える。
CREATE TABLE take_gaps (
  take_id  INTEGER NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
  -- 記録した順。 0 始まり。
  ordinal  INTEGER NOT NULL,
  kind     TEXT    NOT NULL CHECK (kind IN ('dropped', 'discontinuity', 'render_error')),
  position INTEGER NOT NULL,
  PRIMARY KEY (take_id, ordinal)
) STRICT;

-- テイクの無効化は、取りこぼし・不連続・レンダの失敗のどれかが増えたら（人が決めた）。
-- discontinuities は既存のまま（欄の意味は変えていない）。 足すのは残り2つの数と、
-- 固定長の領域に収まらなかった件数。
ALTER TABLE take_metrics ADD COLUMN dropped INTEGER NOT NULL DEFAULT 0;
ALTER TABLE take_metrics ADD COLUMN render_errors INTEGER NOT NULL DEFAULT 0;
ALTER TABLE take_metrics ADD COLUMN gaps_overflowed INTEGER NOT NULL DEFAULT 0;
