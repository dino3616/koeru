//! ライブラリの置き場所を移す（`DEC-PKG-016`、`TR-PKG-37`）。
//!
//! Windows だけ、既定のライブラリ置き場が Roaming（`%APPDATA%`）から
//! Local（`%LOCALAPPDATA%`）へ変わる。 ここは `old` と `new` の2つのパスを
//! 受け取るだけの純粋なファイル操作で、Tauri にも Windows の API にも依存しない
//! ——どの OS でも試験できる（Windows でしか起きない移し替えを、macOS や Linux の
//! CI でも見るため）。「移すべきか」は呼び出し側（`koeru-app`）が決め、
//! ここは「どう移すか」だけを持つ。
//!
//! ## 手順
//!
//! 1. `new` の兄弟に作業場所（`<new のファイル名>.part`）を作る。前回の残りが
//!    あれば消してからにする
//! 2. `old` の木を再帰で写す。ファイルごとに fsync する。symlink は辿らず止める
//! 3. 写した全ファイルを `old` と突き合わせる（大きさとバイト列）
//! 4. 作業場所の中に移し替え済みの印（[`MARKER_FILE_NAME`]）を置いて fsync する
//! 5. 作業場所を `new` へ rename し、親ディレクトリを fsync する
//! 6. `old` を消す
//!
//! 3〜5 のどこで落ちても `new` は現れない（rename が最後）ので、途中の失敗は
//! `old` に触れないまま安全に再試行できる。6 で落ちた場合だけ `new` と `old` の
//! 両方に中身が残るが、`new` 側に印があるので「混ざった」とは区別できる。

use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

/// 移し替え済みの印のファイル名。
///
/// 中身は版の番号だけで、パスは書かない（`AGENTS.md` #3。トレースにパスを
/// 載せない規律を、印のファイルにも及ぼす）。
const MARKER_FILE_NAME: &str = ".relocated";

/// 印の版。 形式を変えたら上げる。
const MARKER_VERSION: &[u8] = b"1";

/// 作業場所（写している途中のディレクトリ）に付ける拡張子。
const PART_SUFFIX: &str = ".part";

/// ライブラリの置き場所を移すときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum RelocateError {
    #[error("入出力に失敗した")]
    Io(#[from] io::Error),

    /// 写す木の中に symlink がある。
    ///
    /// リンク先を写すと、ライブラリの外にあるものを持ち込むことになる。
    /// KOERU は symlink を作らないので、写さずに止める。
    #[error("symlink を含む木は写せない")]
    Symlink,

    /// 写した内容が `old` と一致しない（大きさ、またはバイト列）。
    #[error("写した内容が元と一致しない")]
    Verification,
}

impl koeru_failure::Failure for RelocateError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "relocate.io",
            Self::Symlink => "relocate.symlink",
            Self::Verification => "relocate.verification",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Io(e) => koeru_failure::io_class(e),
            Self::Symlink => koeru_failure::Class::InvalidInput,
            // 突き合わせで見つけた不一致は、書いたはずのものが壊れている状態。
            Self::Verification => koeru_failure::Class::Corrupt,
        }
    }
}

type Result<T> = std::result::Result<T, RelocateError>;

/// 移し替えの結果。
///
/// 失敗ではなく値で返す（`DEC-PLT-038`）。 「移すものが無い」も「両方にある」も
/// 予期できる状態で、呼び出し側が続行を諦める理由にはならない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relocation {
    /// `old` に中身が無い。 移すものが無かった。
    Nothing,
    /// `old` には中身が無く、`new` に既に中身がある。 前に移し終えている。
    AlreadyMoved,
    /// この呼び出しで `old` から `new` へ移した。
    Moved,
    /// `old` にも `new` にも中身があり、混ぜずに両方とも触らず止めた。
    BothHaveContents,
}

impl Relocation {
    /// トレースと記録に渡す固定の名前。 `Debug` の出力を安定した語彷にしない
    /// （variant 名の改名で黙って変わるものを送信層の語彙にしない）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Nothing => "nothing",
            Self::AlreadyMoved => "already_moved",
            Self::Moved => "moved",
            Self::BothHaveContents => "both_have_contents",
        }
    }
}

/// `old` にあるライブラリを `new` へ移す。
///
/// 既に述べた手順（モジュール冒頭）で行う。 `old` にも `new` にも中身があり、
/// `new` に移し替え済みの印が無いときは、混ぜずにどちらにも触らず
/// [`Relocation::BothHaveContents`] を返す。
///
/// 印があるのに `old` がまだ残っているとき（写す・突き合わせる・rename までは
/// 終わっていて、`old` を消すところだけが前回落ちた状態）は、「両方にある」
/// 扱いにせず `old` を消して終える。
///
/// # Errors
///
/// 写す途中の入出力の失敗、symlink を含む木、写した内容が `old` と一致しない
/// とき。 どの失敗でも `new` は作られず、`old` はそのまま残る。
pub fn relocate_library(old: &Path, new: &Path) -> Result<Relocation> {
    let part = part_dir_for(new)?;

    // 前回の残りがあれば消してやり直す。 作業場所は `new` の外側（兄弟）にあり、
    // 消しても `old` にも `new` にも触らない。
    if part.exists() {
        fs::remove_dir_all(&part)?;
    }

    let old_has_contents = has_contents(old)?;
    let new_has_contents = has_contents(new)?;

    if !old_has_contents {
        return Ok(if new_has_contents {
            Relocation::AlreadyMoved
        } else {
            Relocation::Nothing
        });
    }

    if new_has_contents {
        if has_marker(new)? {
            // 写す・突き合わせる・印を置く・rename までは前回で終わっている。
            // `old` を消すところだけが残った状態なので、「両方に中身がある」
            // 扱いにはしない。
            remove_dir_if_present(old)?;
            return Ok(Relocation::Moved);
        }
        return Ok(Relocation::BothHaveContents);
    }

    // ここからは `old` にだけ中身がある通常の移し替え。
    if let Err(e) = copy_verify_and_seal(old, &part) {
        // 写しの失敗は `new` を作らず、`old` にも触らない。作業場所だけ片付ける。
        let _ = fs::remove_dir_all(&part);
        return Err(e);
    }

    if let Some(parent) = new.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(&part, new)?;
    if let Some(parent) = new.parent() {
        fsync_dir(parent)?;
    }

    remove_dir_if_present(old)?;

    Ok(Relocation::Moved)
}

/// `new` の兄弟に作る作業場所の名前。 `new` のファイル名に `.part` を足す。
fn part_dir_for(new: &Path) -> Result<PathBuf> {
    let Some(name) = new.file_name() else {
        return Err(RelocateError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "移し替え先にファイル名が無い",
        )));
    };
    let mut part_name = name.to_os_string();
    part_name.push(PART_SUFFIX);
    Ok(new.with_file_name(part_name))
}

/// 写す・突き合わせる・印を置いて fsync する、をひとまとめにする。
///
/// 呼び出し側がここの失敗をまとめて捕らえ、作業場所を片付けられるようにする。
fn copy_verify_and_seal(old: &Path, part: &Path) -> Result<()> {
    copy_tree(old, part)?;
    verify_tree(old, part)?;
    write_marker(part)?;
    fsync_dir(part)
}

/// ディレクトリが存在し、エントリが1つ以上あるか。
fn has_contents(path: &Path) -> Result<bool> {
    match fs::read_dir(path) {
        Ok(mut it) => match it.next() {
            None => Ok(false),
            Some(Ok(_)) => Ok(true),
            Some(Err(e)) => Err(e.into()),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// `dir` の直下に移し替え済みの印があるか。
fn has_marker(dir: &Path) -> Result<bool> {
    match fs::metadata(dir.join(MARKER_FILE_NAME)) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e.into()),
    }
}

/// 移し替え済みの印を置いて fsync する。
fn write_marker(dir: &Path) -> Result<()> {
    let path = dir.join(MARKER_FILE_NAME);
    fs::write(&path, MARKER_VERSION)?;
    sync_file(&path)
}

/// `src` の木を `dest` へ再帰で写す。 バイトは変えない。
///
/// symlink は辿らない。 [`std::fs::DirEntry::file_type`] は symlink を辿らずに
/// 種類を返すので、これだけで判定できる。
fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_symlink() {
            return Err(RelocateError::Symlink);
        }
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            copy_tree(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
            sync_file(&to)?;
        }
    }
    fsync_dir(dest)
}

/// 写した木が `old` と一致するか（大きさとバイト列）。
fn verify_tree(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            verify_tree(&entry.path(), &to)?;
        } else if ty.is_file() && !files_equal(&entry.path(), &to)? {
            return Err(RelocateError::Verification);
        }
        // symlink は `copy_tree` がここへ来る前に止めている。
    }
    Ok(())
}

/// 2つのファイルが大きさとバイト列で一致するか。
fn files_equal(a: &Path, b: &Path) -> Result<bool> {
    if fs::metadata(a)?.len() != fs::metadata(b)?.len() {
        return Ok(false);
    }
    let mut fa = fs::File::open(a)?;
    let mut fb = fs::File::open(b)?;
    let mut ba = [0_u8; 64 * 1024];
    let mut bb = [0_u8; 64 * 1024];
    loop {
        let na = fa.read(&mut ba)?;
        let nb = fb.read(&mut bb)?;
        if na != nb || ba[..na] != bb[..nb] {
            return Ok(false);
        }
        if na == 0 {
            return Ok(true);
        }
    }
}

/// 書ける形で開き直して fsync する。 入れ物のディレクトリは呼び出し側が fsync する。
fn sync_file(path: &Path) -> Result<()> {
    fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(())
}

/// ディレクトリエントリを永続化する。
///
/// Windows にはディレクトリを開いて fsync する経路が無いので、そこでは何もしない
/// （NTFS のメタデータ更新はジャーナルで守られる。`crate::project::sync_dir` と同じ扱い）。
fn fsync_dir(path: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}

/// ディレクトリがあれば中身ごと消す。 無ければ何もしない。
fn remove_dir_if_present(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用の一時ディレクトリ。プロセス ID と連番で衝突を避ける
    /// （`crate::project` の同名ヘルパーと同じ形。試験どうしで dev-dependency を
    /// 増やさない）。
    fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("koeru-relocate-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
        d
    }

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("親ディレクトリを作れること");
        }
        fs::write(path, bytes).expect("書けること");
    }

    fn read_dir_names(path: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(path)
            .expect("読めること")
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn 両方空なら移すものが無い() {
        let base = tmp("nothing");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        assert_eq!(
            relocate_library(&old, &new).expect("成功"),
            Relocation::Nothing
        );
        assert!(!old.exists());
        assert!(!new.exists());
    }

    #[test]
    fn 通常の移し替えで入れ子とバイト列が同じまま消える() {
        let base = tmp("moved");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");

        write_file(&old.join("project.db"), b"db-bytes");
        write_file(&old.join("audio").join("take-1.wav"), b"wav-bytes-1");
        write_file(
            &old.join("audio").join("nested").join("take-2.wav"),
            b"wav-bytes-2",
        );

        let result = relocate_library(&old, &new).expect("成功");
        assert_eq!(result, Relocation::Moved);

        assert!(!old.exists(), "old は消えること");
        assert_eq!(
            fs::read(new.join("project.db")).expect("読めること"),
            b"db-bytes"
        );
        assert_eq!(
            fs::read(new.join("audio").join("take-1.wav")).expect("読めること"),
            b"wav-bytes-1"
        );
        assert_eq!(
            fs::read(new.join("audio").join("nested").join("take-2.wav")).expect("読めること"),
            b"wav-bytes-2"
        );
        assert!(new.join(MARKER_FILE_NAME).is_file(), "印が残ること");

        // 作業場所は消えていること。
        let part = part_dir_for(&new).expect("作業場所の名前が作れること");
        assert!(!part.exists());
    }

    #[test]
    fn 既に移してあれば何もしない() {
        let base = tmp("already");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        write_file(&new.join("project.db"), b"already-there");

        assert_eq!(
            relocate_library(&old, &new).expect("成功"),
            Relocation::AlreadyMoved
        );
        assert_eq!(
            fs::read(new.join("project.db")).expect("読めること"),
            b"already-there"
        );
        assert!(!old.exists());
    }

    #[test]
    fn 両方に中身があれば混ぜずに止める() {
        let base = tmp("both");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        write_file(&old.join("project.db"), b"old-bytes");
        write_file(&new.join("project.db"), b"new-bytes");

        assert_eq!(
            relocate_library(&old, &new).expect("成功"),
            Relocation::BothHaveContents
        );
        // どちらにも触らない。
        assert_eq!(
            fs::read(old.join("project.db")).expect("読めること"),
            b"old-bytes"
        );
        assert_eq!(
            fs::read(new.join("project.db")).expect("読めること"),
            b"new-bytes"
        );
    }

    #[test]
    fn 作業場所が残った状態からやり直せる() {
        let base = tmp("stale-part");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        write_file(&old.join("project.db"), b"db-bytes");

        // 前回、写している途中で落ちた状態を模す。
        let part = part_dir_for(&new).expect("作業場所の名前が作れること");
        write_file(&part.join("half-written"), b"garbage");

        assert_eq!(
            relocate_library(&old, &new).expect("成功"),
            Relocation::Moved
        );
        assert!(!old.exists());
        assert_eq!(
            fs::read(new.join("project.db")).expect("読めること"),
            b"db-bytes"
        );
        // 前回の残骸は引き継がれない。
        assert_eq!(
            read_dir_names(&new),
            vec![MARKER_FILE_NAME.to_owned(), "project.db".to_owned()]
        );
    }

    #[test]
    fn 印があり_old_が残った状態から_old_を消して終える() {
        let base = tmp("marker-old-left");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");

        // rename までは前回で終わっていて、old の削除だけが残った状態を模す。
        write_file(&new.join("project.db"), b"moved-bytes");
        write_file(&new.join(MARKER_FILE_NAME), MARKER_VERSION);
        write_file(&old.join("project.db"), b"stale-old-bytes");

        assert_eq!(
            relocate_library(&old, &new).expect("成功"),
            Relocation::Moved
        );
        assert!(!old.exists(), "old が消えて終わること");
        assert_eq!(
            fs::read(new.join("project.db")).expect("読めること"),
            b"moved-bytes"
        );
    }

    #[test]
    #[cfg(unix)]
    fn symlink_を含む木は写さず止める() {
        use std::os::unix::fs::symlink;

        let base = tmp("symlink");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        let outside = base.join("outside.txt");
        write_file(&old.join("project.db"), b"db-bytes");
        write_file(&outside, b"outside-bytes");
        symlink(&outside, old.join("link.txt")).expect("symlink を作れること");

        let err = relocate_library(&old, &new).expect_err("止まること");
        assert!(matches!(err, RelocateError::Symlink));
        assert!(!new.exists(), "new は作られないこと");
        assert!(old.join("project.db").is_file(), "old はそのまま残ること");

        // 作業場所も残さない。
        let part = part_dir_for(&new).expect("作業場所の名前が作れること");
        assert!(!part.exists());
    }

    #[test]
    #[cfg(unix)]
    fn 写せない元ファイルがあれば_new_を作らず_old_に触らない() {
        use std::os::unix::fs::PermissionsExt as _;

        let base = tmp("unreadable");
        let old = base.join("old").join("library");
        let new = base.join("new").join("library");
        let unreadable = old.join("unreadable.bin");
        write_file(&unreadable, b"secret");
        fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o000))
            .expect("権限を変えられること");

        let result = relocate_library(&old, &new);
        // 権限で読めなければ入出力の失敗、root で実行していて読めてしまえば
        // 成功する——CI のコンテナが root で走ることがあるための保険。
        match result {
            Ok(Relocation::Moved) => {}
            Err(RelocateError::Io(_)) => {
                assert!(!new.exists(), "new は作られないこと");
                let part = part_dir_for(&new).expect("作業場所の名前が作れること");
                assert!(!part.exists(), "作業場所も残さないこと");
            }
            other => panic!("想定外の結果: {other:?}"),
        }
        // 後始末。読めるようにしないと一時ディレクトリの掃除に失敗する環境がある。
        let _ = fs::set_permissions(&unreadable, fs::Permissions::from_mode(0o644));
    }
}
