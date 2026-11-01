-- 枠（サブバンク）を足し、行を枠に属させる（DEC-RCL-018）。
--
-- 既存のプロジェクトは、作ったときの収録音階ごとに、声色なしの枠を1つずつ持つ。
-- 方式とプリセットは manifest が持っていて SQL からは読めないので、移行の段
-- （ProjectDir::migrate_ledger）が写しの台帳へ migration_manifest に1行置いてから当てる。
-- 置かれていないまま行があると、行の subbank_id が NULL になって落ちる——推し量って
-- 方式を埋めない（DEC-PLT-041）。 行の無い新規の台帳では、置かれていなくてよい。

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE IF NOT EXISTS migration_manifest (
    id        INTEGER PRIMARY KEY CHECK (id = 1),
    method    TEXT    NOT NULL,
    preset_id TEXT
) STRICT;

CREATE TABLE subbanks (
    -- 声色・方式・収録音階を付け替えても変わらない。 消して足し直した枠は別の値になる
    -- （specs/application/schema/shared.graphql の SubbankId）。 AUTOINCREMENT は消した値を使い直さない。
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 声色の名前。 NULL は声色なし。 空文字は持たない。
    voice_color TEXT    CHECK (voice_color IS NULL OR voice_color <> ''),
    -- 収録方式。 音階は枠ごとに1つなので、多音階連続音もここでは連続音になる。
    method      TEXT    NOT NULL CHECK (method IN ('single', 'sequential', 'cvvc')),
    preset_id   TEXT,
    -- 収録音階（MIDI ノート番号）。 既存の行の値をそのまま受けるので、範囲はここで縛らない。
    tone        INTEGER NOT NULL,
    -- 枠の並び（DEC-RCL-018）。
    ordinal     INTEGER NOT NULL,
    -- 人が直した担当範囲（TR-RCL-31）。 直していなければ NULL で、既定は読むたびに求める。
    range_low   INTEGER,
    range_high  INTEGER,
    CHECK ((range_low IS NULL) = (range_high IS NULL)),
    CHECK (range_low IS NULL OR (0 <= range_low AND range_low <= range_high AND range_high <= 127))
) STRICT;

-- 声色・方式・収録音階の組は音源の中で一意（TR-RCL-28）。
CREATE UNIQUE INDEX subbanks_identity ON subbanks(IFNULL(voice_color, ''), method, tone);

INSERT INTO subbanks (voice_color, method, preset_id, tone, ordinal)
SELECT NULL, m.method, m.preset_id, t.tone, ROW_NUMBER() OVER (ORDER BY t.tone) - 1
FROM (SELECT DISTINCT tone FROM rows) AS t
JOIN migration_manifest AS m ON m.id = 1
ORDER BY t.tone;

CREATE TABLE rows_new (
    -- 行 ID。台帳と録音実体の突き合わせはこれで行う（TR-RCL-18）。
    id            TEXT    PRIMARY KEY NOT NULL,
    -- 読み上げるテキスト。日本語のまま（TR-RCL-08）。
    text          TEXT    NOT NULL,
    -- ファイル名の幹。ASCII 固定（DEC-PKG-004）。
    file_stem     TEXT    NOT NULL,
    -- 収録音高（MIDI ノート番号）。 枠の収録音階と同じ値で、枠の音階を付け替えるときに一緒に書き換える。
    tone          INTEGER NOT NULL,
    -- 未録音 / 録音済み / 要録り直し / 除外（TR-RCL-18）。
    state         TEXT    NOT NULL,
    -- 並び順。生成が決定的なので、この順も決定的（TR-RCL-27）。
    ordinal       INTEGER NOT NULL,
    -- 行の出どころ（TR-RCL-18 (g)、DEC-RCL-016）。
    origin        TEXT    NOT NULL DEFAULT 'preset',
    -- 行が属する枠（DEC-RCL-018）。 枠を消すときは、行を先に消す。
    subbank_id    INTEGER NOT NULL REFERENCES subbanks(id),
    UNIQUE (subbank_id, file_stem)
) STRICT;

INSERT INTO rows_new (id, text, file_stem, tone, state, ordinal, origin, subbank_id)
SELECT r.id, r.text, r.file_stem, r.tone, r.state, r.ordinal, r.origin,
       (SELECT s.id FROM subbanks AS s WHERE s.tone = r.tone)
FROM rows AS r;

DROP TABLE rows;
ALTER TABLE rows_new RENAME TO rows;

CREATE INDEX idx_rows_state ON rows(state);
CREATE INDEX idx_rows_subbank ON rows(subbank_id);

CREATE TRIGGER rev_rows_ins AFTER INSERT ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_upd AFTER UPDATE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_del AFTER DELETE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_subbanks_ins AFTER INSERT ON subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_subbanks_upd AFTER UPDATE ON subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_subbanks_del AFTER DELETE ON subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

DROP TABLE migration_manifest;

COMMIT;

PRAGMA foreign_keys = ON;
