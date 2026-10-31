-- ガイドのテンポ（TR-REC-23、DEC-REC-012）。
--
-- 音源に1つ。 行が無ければ既定（koeru_model::guide::GuideTempo::DEFAULT）。
-- 既定も範囲も刻みも SQL に写さない。 規則は koeru_model::guide の1箇所に置く。
CREATE TABLE guide_settings (
    id         INTEGER PRIMARY KEY CHECK (id = 1),
    tempo_bpm  INTEGER NOT NULL
) STRICT;

-- 収録セッションが録ったときのガイドのテンポ（TR-REC-30）。
--
-- テンポを変えたらセッションを切るので、1つのセッションの中では変わらない。
--
-- 既存の行は NULL のまま残す。 この列より前は、収録中にガイドを鳴らす経路が無かった
-- （take_metrics.guide_offset_frames が全テイクで NULL）。 120 で埋めると、
-- 120 BPM の拍に合わせて歌ったテイクに見える。
ALTER TABLE sessions ADD COLUMN guide_tempo_bpm INTEGER;

-- 版（DEC-PLT-043）。 全表にトリガーを置く規則に従う（crates/koeru-core/tests/read_snapshot.rs が見る）。
CREATE TRIGGER rev_guide_settings_ins AFTER INSERT ON guide_settings BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_guide_settings_upd AFTER UPDATE ON guide_settings BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_guide_settings_del AFTER DELETE ON guide_settings BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
