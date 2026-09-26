-- 録る前の予定と、テイクを確定した受領証（DEC-REC-010、project-storage.fsl の ASSUME-8・9）。
--
-- 予定はテイクではない。 テイクの行は今までどおり WAV を確定してから入る（ASSUME-5）。
-- 孤児はこの予定を持つので、どの行・どの収録セッションの録音かを推し量らずに示せる。
CREATE TABLE capture_intents (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- 録音1回の識別子。 画面へ返す CaptureId（specs/application/schema/shared.graphql）。
    capture_id  TEXT    NOT NULL UNIQUE,
    -- 行への外部キーを張らない。 張ると、ファイルを持たずに閉じた予定が残っている行を
    -- 組み直し（DEC-RCL-016）で消せなくなる。 開いている予定の行は、組み直しの側が
    -- 消す対象から外す（Ledger::untaken_rows）。
    row_id      TEXT    NOT NULL,
    session_id  INTEGER NOT NULL REFERENCES sessions(id),
    -- 確定したときの WAV の場所。 プロジェクトの根からの相対パス（takes.rel_path と同じ形）。
    -- 孤児はこれで予定と突き合わせる。 同じ場所を2つの予定に持たせない。
    rel_path    TEXT    NOT NULL UNIQUE,
    declared_at TEXT    NOT NULL,
    -- open      録音に使っている予定。 同時に1つまで（下の索引）
    -- orphaned  確定した WAV を持ったまま、テイクにならずに残った（本人が採るか捨てる）
    -- partial   書きかけ（.wav.part）だけを持って残った
    -- committed テイクの行と一緒に閉じた
    -- discarded テイクにならずに閉じた
    -- abandoned ファイルを持たずに残った予定に、起動時の検証が付けた印。 消さない
    state       TEXT    NOT NULL CHECK (
        state IN ('open', 'orphaned', 'partial', 'committed', 'discarded', 'abandoned')
    ),
    closed_at   TEXT,
    take_id     INTEGER UNIQUE REFERENCES takes(id),
    CHECK ((state = 'committed') = (take_id IS NOT NULL)),
    CHECK ((state IN ('open', 'orphaned', 'partial')) = (closed_at IS NULL))
) STRICT;

-- 単一の書き手。 録音に使っている予定は同時に1つまで。
CREATE UNIQUE INDEX one_open_capture ON capture_intents(state) WHERE state = 'open';
CREATE INDEX idx_capture_intents_row ON capture_intents(row_id);

-- テイクを確定した操作の受領証（DEC-PLT-035）。 予定を閉じ、テイクの行を足すのと
-- 同じトランザクションで書く。 同じ operation_id の送り直しには、これから同じ結果を返す。
CREATE TABLE commit_receipts (
    operation_id TEXT    PRIMARY KEY NOT NULL,
    capture_id   TEXT    NOT NULL UNIQUE REFERENCES capture_intents(capture_id),
    take_id      INTEGER NOT NULL UNIQUE REFERENCES takes(id),
    committed_at TEXT    NOT NULL
) STRICT;
