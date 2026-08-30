-- モノラルの元にするチャンネル（TR-REC-06）。
--
-- **L+R の平均を既定にしない。** 片側にしか信号が無いインタフェースは珍しくなく、
-- 平均すると 6dB 損をする。校正で有意な信号を持つ側を選び、**プロジェクトに固定する。**
--
-- -1 は「全チャンネルを混ぜる」。**本人が選んだときだけ。**
ALTER TABLE sessions ADD COLUMN source_channel INTEGER NOT NULL DEFAULT 0;

-- 校正の時点で選んだチャンネル。**デバイスごとに固定する。**
ALTER TABLE calibrations ADD COLUMN source_channel INTEGER NOT NULL DEFAULT 0;
