-- テイクの名前（収録の画面のテイクの切り替え）。
--
-- NULL は名前を付けていない。 画面は既定の表示（何本目か）を出す。 消すときも NULL へ
-- 戻し、空文字を置かない——空文字を許すと「付けていない」が2通りになる。
-- 表示のためだけの列で、鍵にしない。 この列より前に録ったテイクは NULL のまま。
ALTER TABLE takes ADD COLUMN label TEXT
  CHECK (label IS NULL OR length(label) > 0);
