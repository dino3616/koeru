//! 周波数表（`.frq`）をファイルへ書く（`TR-PKG-05`）。
//!
//! 書式は `koeru_formats::frq` へ移した（`DEC-PLT-034`）。 下の再輸出は既存の
//! `koeru_core::frq` の経路を通すためだけにある。 **移行中。** ここに残るのは
//! ファイルに触るものだけ。

use std::io::Write as _;
use std::path::Path;

pub use koeru_formats::frq::{Frq, FrqError, HOP_SIZE, UNVOICED, frq_path};

/// 周波数表をファイルへ書くときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum FrqWriteError {
    #[error("入出力に失敗した")]
    Io(#[from] std::io::Error),

    /// 書く前にバイト列を作れなかった。
    #[error("周波数表のバイト列を作れない")]
    Format(#[from] FrqError),
}

impl koeru_failure::Failure for FrqWriteError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "frq.io",
            Self::Format(e) => e.code(),
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Io(e) => koeru_failure::io_class(e),
            Self::Format(e) => e.class(),
        }
    }
}

/// ファイルへ書く。fsync してから rename（途中で落ちても半端な表を残さない）。
///
/// # Errors
///
/// f0 と amp の長さが揃っていない、書けない、名前を付け替えられない。
#[tracing::instrument(skip(frq, path))]
pub fn write(frq: &Frq, path: &Path) -> Result<(), FrqWriteError> {
    let bytes = frq.to_bytes()?;
    let tmp = path.with_extension("frq.part");
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_failure::Failure as _;

    /// 書いたファイルは書式のバイト列そのもので、途中の `.part` を残さない。
    #[test]
    fn 書式のバイト列をそのまま書き途中のファイルを残さない() {
        let dir = std::env::temp_dir().join(format!("koeru-frq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("作れること");
        let path = dir.join("a_wav.frq");
        let f = Frq {
            f0: vec![440.0, 0.0],
            amp: vec![0.5, 0.0],
        };

        write(&f, &path).expect("書けること");

        assert_eq!(
            std::fs::read(&path).expect("読めること"),
            f.to_bytes().expect("作れること")
        );
        assert!(!path.with_extension("frq.part").exists());

        let broken = Frq {
            f0: vec![1.0],
            amp: vec![],
        };
        assert_eq!(
            write(&broken, &path).expect_err("拒むこと").code(),
            "frq.length_mismatch",
            "書式の失敗は書式の code のまま返す"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
