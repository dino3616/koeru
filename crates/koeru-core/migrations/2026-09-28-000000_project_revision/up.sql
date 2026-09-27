-- project の版（DEC-PLT-043）。
--
-- 1行だけの表に、単調に増える整数を持つ。 書き手が呼び出しのたびに明示的に
-- 進めるのではなく、既存の全表への AFTER トリガーが進める——書く場所を
-- 足したときに進め忘れても、「全表にトリガーがある」ことを機械で確かめられる
-- （crates/koeru-core/tests/read_snapshot.rs）。トリガーは1回の書き込みトランザクションの
-- 中で複数の行を触りうるので、1回の操作で2以上進むことがある。値は等しいかだけを
-- 比べ、大小や差分を比較には使わない（specs/application/schema/shared.graphql の
-- `Revision`）。
CREATE TABLE project_revision (
  id    INTEGER PRIMARY KEY CHECK (id = 1),
  value INTEGER NOT NULL
) STRICT;

INSERT INTO project_revision (id, value) VALUES (1, 0);

CREATE TRIGGER rev_sessions_ins AFTER INSERT ON sessions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_sessions_upd AFTER UPDATE ON sessions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_sessions_del AFTER DELETE ON sessions BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_rows_ins AFTER INSERT ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_upd AFTER UPDATE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_rows_del AFTER DELETE ON rows BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_recording_order_ins AFTER INSERT ON recording_order BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_recording_order_upd AFTER UPDATE ON recording_order BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_recording_order_del AFTER DELETE ON recording_order BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_row_aliases_ins AFTER INSERT ON row_aliases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_row_aliases_upd AFTER UPDATE ON row_aliases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_row_aliases_del AFTER DELETE ON row_aliases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_take_boundaries_ins AFTER INSERT ON take_boundaries BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_boundaries_upd AFTER UPDATE ON take_boundaries BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_boundaries_del AFTER DELETE ON take_boundaries BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_row_units_ins AFTER INSERT ON row_units BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_row_units_upd AFTER UPDATE ON row_units BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_row_units_del AFTER DELETE ON row_units BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_takes_ins AFTER INSERT ON takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_takes_upd AFTER UPDATE ON takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_takes_del AFTER DELETE ON takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_adopted_takes_ins AFTER INSERT ON adopted_takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_adopted_takes_upd AFTER UPDATE ON adopted_takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_adopted_takes_del AFTER DELETE ON adopted_takes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_oto_values_ins AFTER INSERT ON oto_values BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_oto_values_upd AFTER UPDATE ON oto_values BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_oto_values_del AFTER DELETE ON oto_values BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_presamp_snapshot_ins AFTER INSERT ON presamp_snapshot BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_presamp_snapshot_upd AFTER UPDATE ON presamp_snapshot BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_presamp_snapshot_del AFTER DELETE ON presamp_snapshot BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_review_state_ins AFTER INSERT ON review_state BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_review_state_upd AFTER UPDATE ON review_state BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_review_state_del AFTER DELETE ON review_state BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_take_fingerprints_ins AFTER INSERT ON take_fingerprints BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_fingerprints_upd AFTER UPDATE ON take_fingerprints BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_fingerprints_del AFTER DELETE ON take_fingerprints BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_take_analysis_ins AFTER INSERT ON take_analysis BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_analysis_upd AFTER UPDATE ON take_analysis BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_analysis_del AFTER DELETE ON take_analysis BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_releases_ins AFTER INSERT ON releases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_releases_upd AFTER UPDATE ON releases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_releases_del AFTER DELETE ON releases BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_take_metrics_ins AFTER INSERT ON take_metrics BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_metrics_upd AFTER UPDATE ON take_metrics BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_metrics_del AFTER DELETE ON take_metrics BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_take_gaps_ins AFTER INSERT ON take_gaps BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_gaps_upd AFTER UPDATE ON take_gaps BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_take_gaps_del AFTER DELETE ON take_gaps BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_calibrations_ins AFTER INSERT ON calibrations BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_calibrations_upd AFTER UPDATE ON calibrations BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_calibrations_del AFTER DELETE ON calibrations BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_distribution_ins AFTER INSERT ON distribution BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distribution_upd AFTER UPDATE ON distribution BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_distribution_del AFTER DELETE ON distribution BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_songs_ins AFTER INSERT ON songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_songs_upd AFTER UPDATE ON songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_songs_del AFTER DELETE ON songs BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_song_notes_ins AFTER INSERT ON song_notes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_notes_upd AFTER UPDATE ON song_notes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_song_notes_del AFTER DELETE ON song_notes BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_capture_intents_ins AFTER INSERT ON capture_intents BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_capture_intents_upd AFTER UPDATE ON capture_intents BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_capture_intents_del AFTER DELETE ON capture_intents BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;

CREATE TRIGGER rev_commit_receipts_ins AFTER INSERT ON commit_receipts BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_commit_receipts_upd AFTER UPDATE ON commit_receipts BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
CREATE TRIGGER rev_commit_receipts_del AFTER DELETE ON commit_receipts BEGIN
  UPDATE project_revision SET value = value + 1 WHERE id = 1;
END;
