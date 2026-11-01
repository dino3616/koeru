DROP TRIGGER rev_guide_settings_del;
DROP TRIGGER rev_guide_settings_upd;
DROP TRIGGER rev_guide_settings_ins;

ALTER TABLE sessions DROP COLUMN guide_tempo_bpm;
DROP TABLE guide_settings;
