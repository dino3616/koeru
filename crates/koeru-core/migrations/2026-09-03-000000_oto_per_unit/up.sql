-- oto をモーラごとに持つ（DEC-ALN-013）。
--
-- **1テイクに1つしか持てなかった。** 単独音でも1ファイルに複数モーラが入るので
-- （TR-RCL-03 が1行あたり最大N単位でグルーピングする）、5モーラの行には5つ要る。
-- 同じ WAV を複数のエイリアスが別の位置で指すのが UTAU の単独音の形。
--
-- 既存の行は残す。 エイリアスが分からないので、行の最初の単位を当てる。
-- 分からなければテイクの行 ID をそのまま入れる——**捨てるより、
-- 後から見て「移行で入れた値だ」と分かるほうがよい。**

CREATE TABLE oto_values_new (
    take_id         INTEGER NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
    -- そのエントリのエイリアス（TR-ALN-20 (6) が同一 WAV 内での重複を禁じる）。
    alias           TEXT    NOT NULL,
    offset_ms       REAL    NOT NULL,
    consonant_ms    REAL    NOT NULL,
    cutoff_ms       REAL    NOT NULL,
    preutterance_ms REAL    NOT NULL,
    overlap_ms      REAL    NOT NULL,
    -- 確信度（TR-ALN-24）。機械導出群にのみ付与する。
    confidence      REAL    NOT NULL,
    -- 人が確認したか。違反が残っているエントリは確認済みにしない（DEC-EDT-003）。
    confirmed       INTEGER NOT NULL DEFAULT 0,
    -- 人が手で編集したか。再自動推定から守る（TR-EDT-46）。
    hand_edited     INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (take_id, alias)
) STRICT;

INSERT INTO oto_values_new
    (take_id, alias, offset_ms, consonant_ms, cutoff_ms, preutterance_ms, overlap_ms,
     confidence, confirmed, hand_edited)
SELECT
    o.take_id,
    COALESCE(
        (SELECT ru.kana FROM row_units ru
         JOIN takes t ON t.row_id = ru.row_id
         WHERE t.id = o.take_id
         ORDER BY ru.rowid LIMIT 1),
        (SELECT t.row_id FROM takes t WHERE t.id = o.take_id)
    ),
    o.offset_ms, o.consonant_ms, o.cutoff_ms, o.preutterance_ms, o.overlap_ms,
    o.confidence, o.confirmed, o.hand_edited
FROM oto_values o;

DROP TABLE oto_values;
ALTER TABLE oto_values_new RENAME TO oto_values;
