-- 配布パッケージに出す値（PROFILE-M4）。
--
-- 1プロジェクトに1行。 id を 1 に固定して、2行目を作れなくする。
-- 音源は「プロジェクト1本 = 音源1本」なので、複数行は意味を持たない。
--
-- 表示名はここに置かない。 manifest.toml が持っている（TR-PKG-38）。
-- 配布名は表示名と別（DEC-PKG-008）ので、こちらに置く。
--
-- 本人が書く欄は NULL を許す。 空文字と未記入を分ける——readme は
-- 書いた節だけを出すので（DEC-PKG-011）、区別が要る。
CREATE TABLE distribution (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  distribution_name TEXT NOT NULL,
  profile TEXT NOT NULL,

  -- character.txt / character.yaml に出るもの。
  author TEXT,
  voice TEXT,
  sample TEXT,
  web TEXT,
  version TEXT,

  -- 元画像のまま持つ（DEC-PKG-012）。100×100 の BMP は書き出しのたびに作る。
  -- 変換後を持つと、寸法や畳み方を変えたときに作り直せない。
  icon BLOB,
  portrait BLOB,
  portrait_opacity REAL NOT NULL DEFAULT 1.0,
  portrait_height INTEGER NOT NULL DEFAULT 0,

  -- readme.txt に出るもの（TR-PKG-28）。
  tone_range_note TEXT,
  terms TEXT,
  credit_example TEXT,
  contact TEXT,
  disclaimer TEXT,
  character_note TEXT
);
