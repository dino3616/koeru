//! ライブラリの置き場所の移し替え（`relocate::relocate_library`）を、実際の
//! ファイルシステムの上で確かめる（`SUITE-CORE-005`、`DEC-PKG-016`、`TR-PKG-37`）。
//!
//! ユニット試験（`src/relocate.rs`）と場合分けはほぼ重なるが、こちらは crate の
//! 公開面（`koeru_core::relocate::*`）だけを通して呼ぶ——境界の外から見える形が
//! 変わっていないかを見る。

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use koeru_core::relocate::{Relocation, relocate_library};

/// テスト用の一時ディレクトリ。プロセス ID と連番で衝突を避ける。
fn tmp(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-library-relocation-{}-{tag}-{n}",
        std::process::id()
    ));
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

#[test]
fn 移すものが無ければ何もせずに終える() {
    let base = tmp("nothing");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");

    assert_eq!(
        relocate_library(&old, &new).expect("成功"),
        Relocation::Nothing
    );
    assert!(!old.exists());
    assert!(!new.exists());
}

#[test]
fn 通常の移し替えでプロジェクト一式が同じバイト列のまま_new_へ移る() {
    let base = tmp("normal");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");

    let project = "0193f0c4-0000-0000-0000-000000000000";
    write_file(
        &old.join(project).join("manifest.toml"),
        b"display_name = \"a\"",
    );
    write_file(&old.join(project).join("project.db"), b"\x00sqlite-bytes");
    write_file(
        &old.join(project).join("audio").join("row-1.wav"),
        &vec![0xAB_u8; 4096],
    );

    let result = relocate_library(&old, &new).expect("成功");
    assert_eq!(result, Relocation::Moved);

    assert!(
        !old.exists(),
        "old が消えること（`TR-PKG-37` のライブラリを一本化する）"
    );
    assert_eq!(
        fs::read(new.join(project).join("manifest.toml")).expect("読めること"),
        b"display_name = \"a\""
    );
    assert_eq!(
        fs::read(new.join(project).join("project.db")).expect("読めること"),
        b"\x00sqlite-bytes"
    );
    assert_eq!(
        fs::read(new.join(project).join("audio").join("row-1.wav")).expect("読めること"),
        vec![0xAB_u8; 4096]
    );
}

#[test]
fn 既に移し終えていれば新しく何もしない() {
    let base = tmp("already-moved");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");
    write_file(&new.join("proj").join("manifest.toml"), b"already");

    assert_eq!(
        relocate_library(&old, &new).expect("成功"),
        Relocation::AlreadyMoved
    );
    assert_eq!(
        fs::read(new.join("proj").join("manifest.toml")).expect("読めること"),
        b"already"
    );
}

#[test]
fn 両方に中身があれば混ぜずに_new_を開く判断へ委ねる() {
    let base = tmp("both-have-contents");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");
    write_file(&old.join("proj-old").join("manifest.toml"), b"old");
    write_file(&new.join("proj-new").join("manifest.toml"), b"new");

    assert_eq!(
        relocate_library(&old, &new).expect("成功"),
        Relocation::BothHaveContents
    );
    // 呼び出し側（`koeru-app`）はこの結果を受けて `new` を開き、`old` には触らない
    // (`DEC-PKG-016` の「決めたこと」)。ここでは両方が無事なことだけ見る。
    assert!(old.join("proj-old").join("manifest.toml").is_file());
    assert!(new.join("proj-new").join("manifest.toml").is_file());
}

#[test]
fn 前回の作業場所が残っていても再実行できる() {
    let base = tmp("stale-part");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");
    write_file(&old.join("proj").join("manifest.toml"), b"content");

    // `new` の兄弟に前回の作業場所が残っている状態（クラッシュの模擬）。
    let stale_part = new.with_file_name("library.part");
    write_file(&stale_part.join("half-written.tmp"), b"garbage");

    assert_eq!(
        relocate_library(&old, &new).expect("成功"),
        Relocation::Moved
    );
    assert!(!old.exists());
    assert_eq!(
        fs::read(new.join("proj").join("manifest.toml")).expect("読めること"),
        b"content"
    );
}

#[test]
fn 印があり_old_が残っていても両方にある扱いにせず_old_を消す() {
    let base = tmp("marker-old-left");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");

    // rename までは前回で終わり、old の削除だけが残った状態を模す。
    write_file(&new.join("proj").join("manifest.toml"), b"moved");
    write_file(&new.join(".relocated"), b"1");
    write_file(&old.join("proj").join("manifest.toml"), b"stale");

    assert_eq!(
        relocate_library(&old, &new).expect("成功"),
        Relocation::Moved
    );
    assert!(!old.exists());
    assert_eq!(
        fs::read(new.join("proj").join("manifest.toml")).expect("読めること"),
        b"moved"
    );
}

#[test]
#[cfg(unix)]
fn symlink_を含む木は写さず_old_と_new_のどちらにも触らない() {
    use std::os::unix::fs::symlink;

    let base = tmp("symlink");
    let old = base.join("Roaming").join("library");
    let new = base.join("Local").join("library");
    let outside = base.join("outside.txt");
    write_file(&old.join("proj").join("manifest.toml"), b"content");
    write_file(&outside, b"outside");
    symlink(&outside, old.join("proj").join("link")).expect("symlink を作れること");

    let err = relocate_library(&old, &new).expect_err("止まること");
    assert!(matches!(err, koeru_core::relocate::RelocateError::Symlink));
    assert!(!new.exists());
    assert!(old.join("proj").join("manifest.toml").is_file());
}
