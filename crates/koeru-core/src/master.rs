//! マスター WAV の内容の指紋（`DEC-PLT-053`）。
//!
//! ファイル全体の SHA-256 で、ヘッダも含める。 台帳の `takes.master_sha256` へ置く値で、
//! 確定の直後に1度だけ計算する。 計算し直すのは、この列を足す前に録ったテイクが
//! 初めて要るときだけ（[`crate::db::Ledger::record_master_digest`]）。

use std::fs::File;
use std::io::{self, Read as _};
use std::path::Path;

use crate::fingerprint::{ContentHasher, Digest};

/// 1度に読む大きさ。 マスターは数百 KB から数 MB で、全部をメモリへ載せなくてよい。
const CHUNK_BYTES: usize = 64 * 1024;

/// ファイルの中身の指紋。
///
/// # Errors
///
/// ファイルを開けない、読めない。
pub fn digest_file(path: &Path) -> io::Result<Digest> {
    let mut file = File::open(path)?;
    let mut hasher = ContentHasher::new();
    let mut buf = vec![0_u8; CHUNK_BYTES];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finish());
        }
        hasher.update(&buf[..n]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("koeru-master-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("作れる");
        dir.join(name)
    }

    /// 区切りの大きさをまたぐファイルでも、全体を一度に食わせたものと同じ値になる。
    #[test]
    fn ファイルの指紋は中身そのものの指紋と同じ() {
        let bytes: Vec<u8> = (0..CHUNK_BYTES * 2 + 7).map(|i| (i % 251) as u8).collect();
        let path = tmp("a.wav");
        std::fs::write(&path, &bytes).expect("書ける");
        assert_eq!(digest_file(&path).expect("読める"), Digest::of(&bytes));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn 無いファイルは読めない() {
        assert!(digest_file(&tmp("none.wav")).is_err());
    }
}
