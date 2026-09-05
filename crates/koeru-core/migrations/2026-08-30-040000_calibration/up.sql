-- 入力レベルの校正（TR-REC-14, TR-REC-15）。
--
-- デバイスごとに1つ。 同じプロジェクトを別のマイクで続けることがあり、
-- そのときに前のマイクの値を当てはめると意味が無い。
CREATE TABLE calibrations (
  device_id   TEXT    PRIMARY KEY NOT NULL,

  -- 校正で決めたゲイン（0.0〜1.0）。読み書きできなければ NULL。
  gain        REAL,
  -- hardware / software / unavailable（TR-REC-14）。
  -- software は校正に使えない。 デジタル側で掛けても A/D の手前は変わらない。
  control     TEXT    NOT NULL,

  -- 最後に測ったピーク（dBFS）。
  peak_dbfs   REAL    NOT NULL,
  -- 目標範囲（-12 〜 -6 dBFS）に入ったか。入らなくても収録には進める。
  settled     INTEGER NOT NULL,

  measured_at TEXT    NOT NULL
) STRICT;
