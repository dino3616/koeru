-- 課題曲の内部形式にテンポと既定ピッチベンドを持たせる（TR-SYN-30）。
--
-- > 課題曲は KOERU 内部形式（音符の開始位置 / 長さ / 音高 / 歌詞、テンポ、
-- > 既定ピッチベンド）で持つ。UST / USTX は読み込みの入口であって内部形式ではない
--
-- 持っていなかった。 ノートは持っていたが、テンポとピッチベンドが落ちていて、
-- 読み込んだ UST のテンポが復元できなかった。
ALTER TABLE songs ADD COLUMN tempo_bpm REAL NOT NULL DEFAULT 120.0;

-- 既定のポルタメント長（ミリ秒）。
--
-- 音符ごとのベンドは持たない。 UTAU の PBS/PBW/PBY/PBM は音符ごとの形だが、
-- KOERU の試唱はフラグを既定に固定する（TR-SYN-09）ので、曲に1つで足りる。
ALTER TABLE songs ADD COLUMN default_portamento_ms REAL NOT NULL DEFAULT 0.0;
