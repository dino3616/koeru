-- 録る順のモード（TR-SYN-19）。
--
-- プロジェクトに1行。 モードは音源ごとに1つで、行には付かない。
--
-- **提示順は台帳を書き換えない。** ここが持つのは「次に何を録るか」の並べ方だけで、
-- 録音リストの正準順（TR-RCL-27）も行集合（TR-RCL-18）も動かない。
CREATE TABLE recording_order (
    id      INTEGER PRIMARY KEY CHECK (id = 1),
    -- 'song_bank_first' | 'coverage_efficiency'
    mode    TEXT    NOT NULL DEFAULT 'song_bank_first',
    -- 本人が明示的に切り替えたか（TR-SYN-19 の (b)）。
    --
    -- 立っていると自動では戻さない。 曲バンクが完全になっても、
    -- 本人が選んだモードを上書きしない。
    pinned  INTEGER NOT NULL DEFAULT 0
) STRICT;

INSERT INTO recording_order (id) VALUES (1);
