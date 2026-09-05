-- 書き出し単位のバージョン管理（TR-PKG-44）。
--
-- **リリースレコードは不変。** 書き換えられると「このバージョンで配ったもの」が
-- 後から変わり、受け手の手元にあるパッケージと突き合わせられなくなる。
-- 規律ではなくトリガで止める。
CREATE TABLE releases (
  -- 単調増加する連番。採番は DB が持つ。
  seq          INTEGER PRIMARY KEY NOT NULL,

  -- ユーザーが付けたバージョン文字列。書式は問わない。
  version      TEXT    NOT NULL,
  -- 含めた方式（single / sequential / cvvc / multi_pitch_sequential）。
  method       TEXT    NOT NULL,
  -- 含めたエイリアス数。
  alias_count  INTEGER NOT NULL,
  -- 書き出し前検証の結果。
  validation   TEXT    NOT NULL,

  -- 生成した oto.ini の内容ハッシュ（SHA-256、小文字16進）。
  -- 外部ツールでの編集をここから検出する（TR-PKG-48）。
  oto_hash     TEXT    NOT NULL,
  -- 規約本文のハッシュ。同上。
  terms_hash   TEXT    NOT NULL,

  -- exports/ 配下の名前。同じ名前を二度使わない（過去の ZIP を上書きしない）。
  archive_name TEXT    NOT NULL,
  released_at  TEXT    NOT NULL
) STRICT;

CREATE UNIQUE INDEX releases_archive_name ON releases (archive_name);

-- 過去のリリースを書き換えない・消さない（TR-PKG-44）。
CREATE TRIGGER releases_reject_update
BEFORE UPDATE ON releases
BEGIN
  SELECT RAISE(ABORT, 'release records are immutable');
END;

CREATE TRIGGER releases_reject_delete
BEFORE DELETE ON releases
BEGIN
  SELECT RAISE(ABORT, 'release records are immutable');
END;
