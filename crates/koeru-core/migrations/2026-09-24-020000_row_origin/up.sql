-- 行の出どころ（TR-RCL-18 (g)、DEC-RCL-016）。
--
-- 'preset'（プリセットから生成したフルリスト）か 'repack'（選択から詰め直した行）。
-- 片方の出どころの行を録ったら、もう片方のまだ録っていない行を組み直す
-- ——一度録った綴りを二度読ませない。どちらを組み直すかを決めるのに要る。
--
-- **ID の頭文字で見分けていた。** 詰め直した行は `p` で始まるが、組み直した
-- フルリストの行も同じ生成器で作るので、頭文字では出どころが分からなくなる。
ALTER TABLE rows ADD COLUMN origin TEXT NOT NULL DEFAULT 'preset';
UPDATE rows SET origin = 'repack' WHERE id LIKE 'p%';
