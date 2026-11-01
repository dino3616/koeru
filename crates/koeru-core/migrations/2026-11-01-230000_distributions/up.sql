-- 名前を付けて残す配布物と、配布物ごとの版（DEC-PKG-018、TR-PKG-58〜61）。
--
-- 既存の project は、いまの配布の設定をそのまま「すべて」という配布物1つへ移す。
-- 音源に1つの値（画像・名義・声の提供者・web・サンプル音声・規約）は `distribution` に残り、
-- 配布物ごとに変わるもの（名前・フォルダ名・入れる枠・規約の上書き・下書きの番号）だけが
-- この表に来る。

CREATE TABLE distributions (
    -- 名前や入れる枠を変えても変わらない。 AUTOINCREMENT は消した値を使い直さない
    -- （書き出しの記録が、消した配布物の番号を持ったまま残る）。
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    name          TEXT    NOT NULL CHECK (name <> ''),
    -- 音源ルートフォルダ名（DEC-PKG-008）。 配布物のあいだで重ならない。
    -- NULL は「まだ決めていない」。 既定は表示名から作るので SQL からは作れず、
    -- 開くときに決めて入れる（`Ledger::fill_default_archive_name`）。
    archive_name  TEXT    CHECK (archive_name IS NULL OR archive_name <> ''),
    -- この配布物だけの利用規約。 NULL なら音源の規約（`distribution.terms`）。
    terms         TEXT,
    -- 次に書き出す版の番号（TR-PKG-60）。 NULL は下書きが無いこと。
    draft_version TEXT    CHECK (draft_version IS NULL OR draft_version <> ''),
    -- 配布物の並び。 足した順。
    ordinal       INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX distributions_archive_name ON distributions(archive_name);

-- 配布物が入れる枠と、その配布物の書き出しに限った担当音域（TR-PKG-59）。
-- 枠の担当音域（subbanks.range_low / range_high）は変えない。
CREATE TABLE distribution_subbanks (
    distribution_id INTEGER NOT NULL REFERENCES distributions(id) ON DELETE CASCADE,
    -- 枠を消すと、どの配布物からも外れる。
    subbank_id      INTEGER NOT NULL REFERENCES subbanks(id) ON DELETE CASCADE,
    -- この配布物で直した担当範囲だけを持つ。 直していなければ NULL で、枠の担当音域を書く。
    range_low       INTEGER,
    range_high      INTEGER,
    PRIMARY KEY (distribution_id, subbank_id),
    CHECK ((range_low IS NULL) = (range_high IS NULL)),
    CHECK (range_low IS NULL OR (0 <= range_low AND range_low <= range_high AND range_high <= 127))
) STRICT;

-- 書き出しの記録がどの配布物のものか（TR-PKG-44）。
-- 配布物を消しても記録は残るので、外部キーにしない。 既存の記録は「すべて」のもの。
-- 記録は不変（releases_reject_update）なので、UPDATE で埋めずに既定値で埋める。
ALTER TABLE releases ADD COLUMN distribution_id INTEGER NOT NULL DEFAULT 1;

-- 書き出した時点の採用テイク（TR-PKG-61）。 「最後に書き出したあとで採用が変わった行」の基準。
-- 行やテイクを消しても残るので、外部キーにしない。 移行より前の記録は持たない。
CREATE TABLE release_adoptions (
    release_seq INTEGER NOT NULL,
    row_id      TEXT    NOT NULL,
    take_id     INTEGER NOT NULL,
    PRIMARY KEY (release_seq, row_id)
) STRICT;

CREATE TRIGGER release_adoptions_reject_update
BEFORE UPDATE ON release_adoptions
BEGIN
  SELECT RAISE(ABORT, 'release records are immutable');
END;

CREATE TRIGGER release_adoptions_reject_delete
BEFORE DELETE ON release_adoptions
BEGIN
  SELECT RAISE(ABORT, 'release records are immutable');
END;

-- いまの設定を「すべて」へ移す。
--
-- フォルダ名は、あれば引き継ぐ。 版は、書いてあって、書き出し済みの番号と重ならないときだけ
-- 下書きにする（下書きは書き出した番号と重ねない）。 何も書いていないとき、書き出していなければ
-- 1.0.0、書き出していれば下書き無し（番号を省いて書き出せば、最後の版の次の番号が出る）。
INSERT INTO distributions (name, archive_name, terms, draft_version, ordinal)
VALUES (
    'すべて',
    (SELECT NULLIF(distribution_name, '') FROM distribution WHERE id = 1),
    NULL,
    CASE
        WHEN (SELECT NULLIF(version, '') FROM distribution WHERE id = 1) IS NOT NULL THEN
            CASE
                WHEN (SELECT version FROM distribution WHERE id = 1)
                     IN (SELECT version FROM releases) THEN NULL
                ELSE (SELECT version FROM distribution WHERE id = 1)
            END
        WHEN EXISTS (SELECT 1 FROM releases) THEN NULL
        ELSE '1.0.0'
    END,
    0
);

-- 「すべて」は、いまある枠をすべて入れる（TR-PKG-58）。
INSERT INTO distribution_subbanks (distribution_id, subbank_id)
SELECT (SELECT MIN(id) FROM distributions), id FROM subbanks;

-- 版（DEC-PLT-043）。 全表にトリガーを置く規則に従う。 移行の挿入では動かさないよう、最後に張る。
CREATE TRIGGER rev_distributions_ins AFTER INSERT ON distributions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distributions_upd AFTER UPDATE ON distributions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distributions_del AFTER DELETE ON distributions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_distribution_subbanks_ins AFTER INSERT ON distribution_subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distribution_subbanks_upd AFTER UPDATE ON distribution_subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distribution_subbanks_del AFTER DELETE ON distribution_subbanks BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_release_adoptions_ins AFTER INSERT ON release_adoptions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_release_adoptions_upd AFTER UPDATE ON release_adoptions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_release_adoptions_del AFTER DELETE ON release_adoptions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
