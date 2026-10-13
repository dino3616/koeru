-- 取り込んだ曲の原文と、取り込みの受領証（DEC-PLT-054）。
--
-- 曲の行（songs / song_notes）は読み取った結果で、持ち込んだファイルそのものではない。
-- パーサを直したとき、原文が無いと取り込み直せない。 原文は project に残し、
-- 同じ operation_id の送り直しには、ここから同じ結果（曲の ID）を返す。
--
-- この移行より前に取り込んだ曲は原文を持たない（ファイルが残っていない）。 作り直さない。
CREATE TABLE song_imports (
  -- 取り込みを確定した操作の識別子（DEC-PLT-035 の operationId）。 振るのは呼び出し側。
  operation_id   TEXT PRIMARY KEY NOT NULL,
  -- ファイルの名前（ディレクトリは含めない）。 形式（UST / USTX）と題の候補を決め直すのに要る。
  file_name      TEXT NOT NULL,
  -- 渡されたバイト列そのまま。 符号化も改行も直さない。
  source_bytes   BLOB NOT NULL,
  -- 要求（ファイル名・バイト列・本人が決めた題）の SHA-256。 同じ operation_id で
  -- 別の要求が来たときに、送り直しと区別する。
  request_digest TEXT NOT NULL,
  imported_at    TEXT NOT NULL
) STRICT;

-- 1回の取り込みが作った曲。 USTX は1トラックが1曲なので、1回に複数入る。
CREATE TABLE song_import_songs (
  operation_id TEXT    NOT NULL REFERENCES song_imports(operation_id),
  -- ファイルの中の並び（0 始まり）。 返す順をこれで決める。
  ordinal      INTEGER NOT NULL,
  -- 曲が2つの取り込みから来ることは無い。
  song_id      TEXT    NOT NULL UNIQUE REFERENCES songs(id),
  PRIMARY KEY (operation_id, ordinal)
) STRICT;

-- 版（DEC-PLT-043）。 全表にトリガーを置く規則に従う（crates/koeru-core/tests/read_snapshot.rs が見る）。
CREATE TRIGGER rev_song_imports_ins AFTER INSERT ON song_imports BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_imports_upd AFTER UPDATE ON song_imports BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_imports_del AFTER DELETE ON song_imports BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_song_import_songs_ins AFTER INSERT ON song_import_songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_import_songs_upd AFTER UPDATE ON song_import_songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_import_songs_del AFTER DELETE ON song_import_songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
