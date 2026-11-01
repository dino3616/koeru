-- 配布物は「すべて」の1つへ畳まない。 表ごと捨てる。
-- フォルダ名と下書きの番号を `distribution` へ戻す（先頭の配布物のものだけ）。

UPDATE distribution
SET distribution_name = COALESCE(
        (SELECT archive_name FROM distributions ORDER BY ordinal, id LIMIT 1),
        distribution_name),
    version = (SELECT draft_version FROM distributions ORDER BY ordinal, id LIMIT 1)
WHERE id = 1;

DROP TABLE release_adoptions;
DROP TABLE distribution_subbanks;
DROP TABLE distributions;

ALTER TABLE releases DROP COLUMN distribution_id;
