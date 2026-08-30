//! 保存先の残量（`TR-REC-24`、`REQ-REC-110`）。
//!
//! **収録の前に見積もって、入る分だけ録らせる。**
//! 3時間の収録の途中でディスクが埋まると、その日の作業を失う。
//!
//! ここは OS を直接叩く。ドメイン層（`koeru-core`）は OS に依存しないと決めてあるので、
//! **この問い合わせはアプリケーション層が持つ。**

use std::path::Path;

/// 1行あたりに見込む収録時間（秒）。
///
/// **実測が入るまでの暫定値**（`TR-RCL-11` の所要時間は実測待ち）。
/// 単独音は1行5単位で、読み上げに 12 秒。**録り直しを2回ぶん見込む。**
const SECONDS_PER_ROW: u64 = 12 * 3;

/// 1サンプルのバイト数。**マスターは 32bit float**（`DEC-REC-003`）。
const BYTES_PER_SAMPLE: u64 = 4;

/// 見積もりに掛ける余裕。**ぴったりで許可しない。**
/// 周波数表・サムネイル・DB・書き出しの控えがこの上に載る。
const HEADROOM: u64 = 3;

/// 残り `rows` 行を録り切るのに要るバイト数。
#[must_use]
pub const fn required_bytes(rows: u64, sample_rate_hz: u32) -> u64 {
    rows.saturating_mul(SECONDS_PER_ROW)
        .saturating_mul(sample_rate_hz as u64)
        .saturating_mul(BYTES_PER_SAMPLE)
        .saturating_mul(HEADROOM)
        / 2
}

/// 保存先の空き容量（バイト）。
///
/// **取れなければ 0 を返さない。** 0 にすると「足りない」と判定され、
/// 残量が読めないだけの環境で収録できなくなる。`None` を返して呼び出し側に決めさせる。
#[must_use]
pub fn available_bytes(path: &Path) -> Option<u64> {
    platform::available_bytes(path)
}

#[cfg(unix)]
mod platform {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    pub(super) fn available_bytes(path: &Path) -> Option<u64> {
        let c = CString::new(path.as_os_str().as_bytes()).ok()?;
        // SAFETY: すべてゼロで初期化された `statvfs` は妥当な初期状態。
        let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` は NUL 終端の有効なパス、`st` は書き込み先として渡す。
        let rc = unsafe { libc::statvfs(c.as_ptr(), &raw mut st) };
        if rc != 0 {
            return None;
        }
        // **`f_bavail` を使う。** `f_bfree` には root だけが使える予約分が入る。
        // **`statvfs` の各欄の型は OS ごとに違う**（macOS は f_bavail が u32、
        // Linux は u64、f_frsize も同様に食い違う）。`try_from` なら
        // どちらでも同じ式で通るが、既に u64 の側では冗長だと lint が言う。
        // **片方に合わせて書き分けると、もう片方で壊れる。**
        #[allow(
            clippy::useless_conversion,
            clippy::unnecessary_fallible_conversions,
            reason = "欄の型が OS ごとに違うので、両方で通る書き方を採る"
        )]
        let (frsize, bavail) = (
            u64::try_from(st.f_frsize).unwrap_or(0),
            u64::try_from(st.f_bavail).unwrap_or(0),
        );
        Some(frsize.saturating_mul(bavail))
    }
}

#[cfg(windows)]
mod platform {
    use std::path::Path;

    pub(super) fn available_bytes(_path: &Path) -> Option<u64> {
        // **Windows のバックエンドはまだ無い**（DEC-REC-001 で後回しと決めた）。
        // ここを 0 で埋めると「足りない」判定になるので、`None` にしておく。
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 残り行数に比例して見積もりが増える() {
        let one = required_bytes(1, 48_000);
        let ten = required_bytes(10, 48_000);
        assert!(one > 0);
        assert_eq!(ten, one * 10);
    }

    #[test]
    fn 全部録り終えていれば要らない() {
        assert_eq!(required_bytes(0, 48_000), 0);
    }

    /// **ぴったりで許可しない。** 実データより多めに見積もる。
    #[test]
    fn 見積もりは実データより余裕を持つ() {
        // 1行 = 12秒 × 48kHz × 4バイト = 約 2.3MB。
        let bare = 12 * 48_000 * 4;
        assert!(required_bytes(1, 48_000) > bare, "余裕が載ること");
    }

    #[test]
    fn 残量を引ける() {
        let got = available_bytes(&std::env::temp_dir());
        #[cfg(unix)]
        assert!(got.is_some_and(|b| b > 0), "unix では引けること");
        #[cfg(not(unix))]
        let _ = got;
    }

    /// **引けないことと 0 を混同しない。**
    #[test]
    fn 引けないパスは無しを返す() {
        assert_eq!(available_bytes(Path::new("/存在しない/場所/x")), None);
    }
}
