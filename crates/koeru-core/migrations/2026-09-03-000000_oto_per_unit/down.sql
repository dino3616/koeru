-- モーラごとの oto を、1テイク1つへ戻す（DEC-ALN-013 の巻き戻し）。
--
-- 戻すと情報が落ちる。 1テイクに複数のエントリがあれば、
-- エイリアス順で最初の1つだけが残る。

CREATE TABLE oto_values_old (
    take_id         INTEGER PRIMARY KEY NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
    offset_ms       REAL    NOT NULL,
    consonant_ms    REAL    NOT NULL,
    cutoff_ms       REAL    NOT NULL,
    preutterance_ms REAL    NOT NULL,
    overlap_ms      REAL    NOT NULL,
    confidence      REAL    NOT NULL,
    confirmed       INTEGER NOT NULL DEFAULT 0,
    hand_edited     INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO oto_values_old
SELECT take_id, offset_ms, consonant_ms, cutoff_ms, preutterance_ms, overlap_ms,
       confidence, confirmed, hand_edited
FROM oto_values
GROUP BY take_id
HAVING alias = MIN(alias);

DROP TABLE oto_values;
ALTER TABLE oto_values_old RENAME TO oto_values;
