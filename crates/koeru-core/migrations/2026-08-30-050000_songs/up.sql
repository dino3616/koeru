-- 課題曲（TR-RCL-12）。
--
-- 曲バンクを持たない。 同梱は初回のとっかかりに要る最小限で、
-- 主経路は本人が持ち込む UST / USTX。
-- 持ち込んだ曲データは配布パッケージに含めないので、ここに置いて終わり。
CREATE TABLE songs (
  id          TEXT    PRIMARY KEY NOT NULL,
  title       TEXT    NOT NULL,

  -- 出典と許諾（TR-RCL-12 (f)）。
  source      TEXT    NOT NULL,
  license     TEXT    NOT NULL,

  -- 同梱分か、本人が持ち込んだものか。 同梱分は本人が外せる。
  bundled     INTEGER NOT NULL,
  -- 曲バンクに入っているか。外しても曲そのものは残す。
  in_bank     INTEGER NOT NULL,

  added_at    TEXT    NOT NULL
) STRICT;

-- ノート（TR-RCL-12 (a)(b)）。
--
-- 任意のノート群を選んで目標にできる（サビだけ、など）ので、
-- 位置で切り出せるように ordinal を持つ。
CREATE TABLE song_notes (
  song_id  TEXT    NOT NULL REFERENCES songs(id) ON DELETE CASCADE,
  ordinal  INTEGER NOT NULL,
  lyric    TEXT    NOT NULL,
  midi     INTEGER NOT NULL,
  ticks    INTEGER NOT NULL,
  PRIMARY KEY (song_id, ordinal)
) STRICT;
