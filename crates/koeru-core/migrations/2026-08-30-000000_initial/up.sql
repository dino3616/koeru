-- KOERU のプロジェクト DB。
--
-- **状態の単一の真実**（TR-REC-31）。`masters/` のファイル存在をスキャンして導出しない。
-- 試唱の可否・再開位置・書き出しの可否をすべてここから決める。

-- 収録セッション。**録音条件のスナップショット**（TR-REC-30 / TR-REC-13）。
CREATE TABLE sessions (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    started_at        TEXT    NOT NULL,
    -- 入力デバイスの永続識別子（TR-REC-03）。**表示名は保存しない。**
    device_id         TEXT    NOT NULL,
    sample_rate_hz    INTEGER NOT NULL,
    channels          INTEGER NOT NULL,
    -- OS 側の音声加工の状態（TR-REC-08 / TR-REC-11）。列挙できなければ 'unknown'。
    effects_state     TEXT    NOT NULL,
    -- 実際に接続した経路の種別（TR-REC-12）。
    route             TEXT    NOT NULL
) STRICT;

-- 録音リストの行。**プリセットから生成した全行**（TR-RCL-18）。
CREATE TABLE rows (
    -- 行 ID。**台帳と録音実体の突き合わせはこれで行う**（TR-RCL-18）。
    id            TEXT    PRIMARY KEY NOT NULL,
    -- 読み上げるテキスト。日本語のまま（TR-RCL-08）。
    text          TEXT    NOT NULL,
    -- ファイル名の幹。ASCII 固定（DEC-PKG-004）。
    file_stem     TEXT    NOT NULL,
    -- 収録音高（MIDI ノート番号）。単音階なら全行同じ。
    tone          INTEGER NOT NULL,
    -- 未録音 / 録音済み / 要録り直し / 除外（TR-RCL-18）。
    state         TEXT    NOT NULL,
    -- 並び順。**生成が決定的なので、この順も決定的**（TR-RCL-27）。
    ordinal       INTEGER NOT NULL,
    UNIQUE (file_stem, tone)
) STRICT;

-- 行が生む収録単位。**カバレッジはここから導出し、二重に保持しない**（TR-RCL-18）。
CREATE TABLE row_units (
    row_id    TEXT    NOT NULL REFERENCES rows(id) ON DELETE CASCADE,
    kana      TEXT    NOT NULL,
    consonant TEXT    NOT NULL,
    vowel     TEXT    NOT NULL,
    PRIMARY KEY (row_id, kana)
) STRICT;

-- テイク。**世代として積み、削除も上書きもしない**（TR-REC-21）。
--
-- **行が入るのは、ファイルが確定したあとだけ**（project-storage.fsl の
-- finalize_file → commit_take）。逆順にすると、ファイルの無い行が DB に残る。
CREATE TABLE takes (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    row_id     TEXT    NOT NULL REFERENCES rows(id) ON DELETE CASCADE,
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    -- masters/ からの相対パス。**確定済みのファイルを指す。**
    rel_path   TEXT    NOT NULL UNIQUE,
    frames     INTEGER NOT NULL,
    recorded_at TEXT   NOT NULL,
    -- 取りこぼしを検出したテイクは無効（TR-REC-07）。
    invalid    INTEGER NOT NULL DEFAULT 0,
    -- 世代。同じ行の中で単調に増える。
    generation INTEGER NOT NULL
) STRICT;

-- 採用テイク。**行ごとに高々1つ。** 切り替えてもカバレッジは変わらない（TR-RCL-25）。
CREATE TABLE adopted_takes (
    row_id  TEXT    PRIMARY KEY NOT NULL REFERENCES rows(id) ON DELETE CASCADE,
    take_id INTEGER NOT NULL REFERENCES takes(id)
) STRICT;

-- oto の5値。テイクごとに1組（TR-EDT-01 は絶対サンプル位置だが、
-- ここは oto.ini との境界なのでミリ秒で持つ）。
CREATE TABLE oto_values (
    take_id         INTEGER PRIMARY KEY NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
    offset_ms       REAL    NOT NULL,
    consonant_ms    REAL    NOT NULL,
    cutoff_ms       REAL    NOT NULL,
    preutterance_ms REAL    NOT NULL,
    overlap_ms      REAL    NOT NULL,
    -- 確信度（TR-ALN-24）。**機械導出群にのみ付与する。**
    confidence      REAL    NOT NULL,
    -- 人が確認したか。**違反が残っているエントリは確認済みにしない**（DEC-EDT-003）。
    confirmed       INTEGER NOT NULL DEFAULT 0,
    -- 人が手で編集したか。**再自動推定から守る**（TR-EDT-46）。
    hand_edited     INTEGER NOT NULL DEFAULT 0
) STRICT;

CREATE INDEX idx_takes_row ON takes(row_id);
CREATE INDEX idx_rows_state ON rows(state);
