-- 同じ音階の枠が2つあり、同じファイル名の幹を持つ行があると、元の一意制約に戻せずに落ちる。

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE rows_old (
    id            TEXT    PRIMARY KEY NOT NULL,
    text          TEXT    NOT NULL,
    file_stem     TEXT    NOT NULL,
    tone          INTEGER NOT NULL,
    state         TEXT    NOT NULL,
    ordinal       INTEGER NOT NULL,
    origin        TEXT    NOT NULL DEFAULT 'preset',
    UNIQUE (file_stem, tone)
) STRICT;

INSERT INTO rows_old (id, text, file_stem, tone, state, ordinal, origin)
SELECT id, text, file_stem, tone, state, ordinal, origin FROM rows;

DROP TABLE rows;
ALTER TABLE rows_old RENAME TO rows;

CREATE INDEX idx_rows_state ON rows(state);

CREATE TRIGGER rev_rows_ins AFTER INSERT ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_upd AFTER UPDATE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_del AFTER DELETE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

DROP TRIGGER rev_subbanks_del;
DROP TRIGGER rev_subbanks_upd;
DROP TRIGGER rev_subbanks_ins;

DROP TABLE subbanks;

COMMIT;

PRAGMA foreign_keys = ON;
