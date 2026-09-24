-- 無声破裂音の分岐不一致（TR-ALN-16 / 17、DEC-ALN-018）。
--
-- 閉鎖の無音を前提にした切り方が、実際には声の途中で切れていた印。
-- 確認キューの先頭へ回す。状態（`state`）とは別に持つ——状態機械
-- （align-review.fsl）には手を入れない。
--
-- 既定は 0。 この列より前に推定したエントリは、推定し直すまで検証されていない。
ALTER TABLE oto_values ADD COLUMN branch_mismatch INTEGER NOT NULL DEFAULT 0;
