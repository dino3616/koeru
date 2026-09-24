-- 音符の前に置く休み（ティック、TR-RCL-12）。
--
-- **休符を落としていた。** UST の `R` も USTX のノート間の空きも捨てていたので、
-- 取り込んだ曲が詰まって鳴り、元と違うリズムになった。
--
-- 休符を音符として持たない。 休みは歌詞もモーラも持たないので、
-- 音符の並びに混ぜると被覆の計算に入ってしまう。
ALTER TABLE song_notes ADD COLUMN rest_ticks INTEGER NOT NULL DEFAULT 0;
