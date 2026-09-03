-- キャプチャからマスターへの変換を記録する（TR-REC-02）。
--
-- > 実際に開けた形式とネイティブレートの一致／不一致をメタデータに記録し、
-- > **上流の変換の有無は [Unknown] と明記する。**
-- > 使用したリサンプラの識別子とバージョンを記録する
--
-- **`sample_rate_hz` はネイティブレートのまま。** マスターは常に 44100 なので、
-- 両方を持って初めて「変換したかどうか」が後から分かる。
--
-- **既存の行は `unknown` を入れる。** 変換が実装される前に録られたもので、
-- **マスターがネイティブレートのまま書かれている**（DEC-REC-006）。
-- 空欄にすると「変換していない」と読めてしまう。

ALTER TABLE sessions ADD COLUMN master_rate_hz INTEGER NOT NULL DEFAULT 0;
ALTER TABLE sessions ADD COLUMN resampler TEXT NOT NULL DEFAULT 'unknown';
-- 上流（ドライバ・APO）の変換の有無。**[Unknown] と明記する**（TR-REC-02）。
ALTER TABLE sessions ADD COLUMN upstream_conversion TEXT NOT NULL DEFAULT 'unknown';
