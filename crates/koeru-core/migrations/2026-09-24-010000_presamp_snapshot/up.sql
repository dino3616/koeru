-- 綴りの表の写し（TR-SYN-36、DEC-SYN-013）。
--
-- presamp.ini はプロジェクトを作るときに選ばせて固定する。 解決・被覆・書き出しは
-- この写しを読み、音源フォルダの presamp.ini は読まない——フォルダのものを読むと、
-- あとから書き換えられた表で、作ったときの表で書いた台帳の綴りを数えることになる。
--
-- 1行しか持たない。 行が無いのは、この列より前に作ったプロジェクト。
-- そのときは同梱の既定が写しになる（作ったときの台帳は既定の表で書かれている）。
CREATE TABLE presamp_snapshot (
    id   INTEGER PRIMARY KEY CHECK (id = 1),
    text TEXT    NOT NULL
) STRICT;
