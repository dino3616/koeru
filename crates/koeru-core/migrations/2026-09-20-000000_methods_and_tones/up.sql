-- 方式を広げるための3つの表（PROFILE-M5）。
--
-- `row_units` は触らない。 あれは「行が生む収録単位」の集合で、連続音の行に
-- 同じ仮名が2度出ても集合としては変わらない。語順は `rows.text` が持っている。

-- 行が生むエイリアス（TR-RCL-18、TR-PKG-22）。
--
-- **単独音では仮名と同じだが、連続音と CVVC では違う。** 「あ か」の行は
-- `- あ` と `a か` を生む。書き出せる方式の判定はエイリアスの被覆で決まるので
-- （TR-PKG-23）、仮名の集合では足りない。
--
-- 行の中で同じエイリアスは2度出ない（TR-ALN-20 (6) が同一 WAV 内の重複を禁じる）。
-- 生成器がそれを保証していて、ここの主鍵がその写し。
CREATE TABLE row_aliases (
    row_id  TEXT    NOT NULL REFERENCES rows(id) ON DELETE CASCADE,
    alias   TEXT    NOT NULL,
    -- 行の中での初出位置。並びを決定的にするために持つ（TR-RCL-27）。
    ordinal INTEGER NOT NULL,
    PRIMARY KEY (row_id, alias)
) STRICT;

CREATE INDEX idx_row_aliases_alias ON row_aliases(alias);

-- アライメントが出した境界（TR-ALN-34）。
--
-- **5値ではなく境界を持つ。** 5値は境界と規約プリセットから導く派生物で
-- （TR-ALN-13 の三分法）、規約を変えたら作り直せる。境界を捨てていたので、
-- プリセットを編集するだけで再アライメントが走っていた——TR-ALN-23 の
-- 「再アライメントを要求しない」と食い違ったまま出荷されていた。
--
-- 下位方式への書き出し（TR-PKG-24）も、ここから5値を作り直す。
CREATE TABLE take_boundaries (
    take_id        INTEGER NOT NULL REFERENCES takes(id) ON DELETE CASCADE,
    -- そのモーラのエイリアス。`oto_values` と同じ鍵で引ける。
    alias          TEXT    NOT NULL,
    -- 発声開始。
    voice_start_ms REAL    NOT NULL,
    -- 子音から母音への境界。母音始まりなら voice_start と同じ。
    vowel_start_ms REAL    NOT NULL,
    -- 母音の定常区間終端。
    vowel_end_ms   REAL    NOT NULL,
    PRIMARY KEY (take_id, alias)
) STRICT;

-- 既にある行のエイリアスを埋める（TR-RCL-18）。
--
-- **埋めていなかった。** この移行より前のプロジェクトは単独音しか作れず、
-- 単独音のエイリアスは仮名そのもの。被覆も書き出しも `row_aliases` だけを
-- 読むようになったので、埋めないと**録り終えた音源が「全部足りない」に
-- 変わり、書き出せなくなる。**
--
-- 単独音だと決め打ってよい。 この移行より前に他の方式は作れなかった。
-- これ以降の行は `install_reclist_for_tones` が方式ごとの綴りで入れる。
--
-- 並びは `rows.text` の中での初出順に合わせられないので、仮名順で振る。
-- `ordinal` は行の中の並びを決定的にするためだけのもので、
-- 単独音では綴りが仮名と同じなので、どちらでも同じ集合になる。
INSERT INTO row_aliases (row_id, alias, ordinal)
SELECT row_id, kana, ROW_NUMBER() OVER (PARTITION BY row_id ORDER BY kana) - 1
FROM row_units;
