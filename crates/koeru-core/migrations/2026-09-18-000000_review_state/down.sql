DROP TABLE take_fingerprints;
DROP TABLE review_state;
ALTER TABLE oto_values DROP COLUMN pinned_overlap;
ALTER TABLE oto_values DROP COLUMN pinned_preutterance;
ALTER TABLE oto_values DROP COLUMN pinned_cutoff;
ALTER TABLE oto_values DROP COLUMN pinned_consonant;
ALTER TABLE oto_values DROP COLUMN pinned_offset;
ALTER TABLE oto_values DROP COLUMN state;
