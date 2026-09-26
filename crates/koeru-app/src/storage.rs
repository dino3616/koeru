//! 保存先の残量（`TR-REC-24`、`REQ-REC-110`）。
//!
//! 収録の前に見積もって、入る分だけ録らせる。
//! 3時間の収録の途中でディスクが埋まると、その日の作業を失う。
//!
//! ここは OS を直接叩く。ドメイン層（`koeru-core`）は OS に依存しないと決めてあるので、
//! この問い合わせはアプリケーション層が持つ。

use std::path::Path;

/// 1行あたりに見込む収録時間（秒）。
///
/// 実測が入るまでの暫定値（`TR-RCL-11` の所要時間は実測待ち）。
/// 単独音は1行5単位で、読み上げに 12 秒。録り直しを2回ぶん見込む。
const SECONDS_PER_ROW: u64 = 12 * 3;

/// 1サンプルのバイト数。マスターは 32bit float（`DEC-REC-003`）。
const BYTES_PER_SAMPLE: u64 = 4;

/// 見積もりに掛ける余裕。ぴったりで許可しない。
/// 周波数表・サムネイル・DB・書き出しの控えがこの上に載る。
///
/// 1.5 倍。 整数で持つために分子と分母に分けてある——
/// `HEADROOM_NUM / HEADROOM_DEN` を掛けるので、`NUM` だけを見て倍率と読まないこと。
const HEADROOM_NUM: u64 = 3;
const HEADROOM_DEN: u64 = 2;

/// 残り `rows` 行を録り切るのに要るバイト数。
#[must_use]
pub const fn required_bytes(rows: u64, sample_rate_hz: u32) -> u64 {
    rows.saturating_mul(SECONDS_PER_ROW)
        .saturating_mul(sample_rate_hz as u64)
        .saturating_mul(BYTES_PER_SAMPLE)
        .saturating_mul(HEADROOM_NUM)
        / HEADROOM_DEN
}

/// この残量で何行ぶん録れるか（`TR-REC-41`）。
///
/// 「足りません」だけでは、何を削れば足りるのか分からない。
/// その残量で録りきれる件数を出して、判断できる形にする。
#[must_use]
pub const fn rows_that_fit(available_bytes: u64, sample_rate_hz: u32) -> u64 {
    let per_row = required_bytes(1, sample_rate_hz);
    if per_row == 0 {
        return 0;
    }
    available_bytes / per_row
}

/// 保存先の空き容量（バイト）。
///
/// 取れなければ 0 を返さない。 0 にすると「足りない」と判定され、
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
        // `f_bavail` を使う。 `f_bfree` には root だけが使える予約分が入る。
        // `statvfs` の各欄の型は OS ごとに違う（macOS は f_bavail が u32、
        // Linux は u64、f_frsize も同様に食い違う）。`try_from` なら
        // どちらでも同じ式で通るが、既に u64 の側では冗長だと lint が言う。
        // 片方に合わせて書き分けると、もう片方で壊れる。
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
        // Windows のバックエンドはまだ無い（`DEC-REC-001` で後回しと決めた）。
        // ここを 0 で埋めると「足りない」判定になるので、`None` にしておく。
        None
    }
}

/// 内蔵ドライブか、ネットワーク上か FAT 系かを見分ける（`DEC-PKG-016`）。
///
/// 耐障害を約束するのは内蔵ドライブの APFS / HFS+ / NTFS / ext4 系だけ。
/// ネットワークドライブ（SMB・NFS・AFP・WebDAV）と FAT 系（FAT32・exFAT）は
/// rename の原子性や fsync が保証されず、抜かれたときや電源断で壊れやすい。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsKind {
    Apfs,
    Hfs,
    Ntfs,
    Ext4,
    Btrfs,
    Xfs,
    /// FAT32 か exFAT。
    Fat,
    /// SMB・NFS・AFP・WebDAV など。
    Network,
    /// 上のどれとも判定できない。 「約束の内」を意味しない——分からないものは
    /// 約束の外として扱う。
    Unknown,
}

impl FsKind {
    /// 内蔵ドライブの、耐障害を約束したファイルシステムか（`DEC-PKG-016`）。
    #[must_use]
    pub const fn is_promised(self) -> bool {
        matches!(
            self,
            Self::Apfs | Self::Hfs | Self::Ntfs | Self::Ext4 | Self::Btrfs | Self::Xfs
        )
    }

    /// トレースに渡す固定の名前。 `Debug` の出力を送信層の語彙にしない
    /// （variant 名の改名で黙って変わるものを固定語彙として送らない）。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Apfs => "apfs",
            Self::Hfs => "hfs",
            Self::Ntfs => "ntfs",
            Self::Ext4 => "ext4",
            Self::Btrfs => "btrfs",
            Self::Xfs => "xfs",
            Self::Fat => "fat",
            Self::Network => "network",
            Self::Unknown => "unknown",
        }
    }
}

impl Default for FsKind {
    /// まだ調べていない・調べられなかったときの既定値。
    fn default() -> Self {
        Self::Unknown
    }
}

/// `path` を含むファイルシステムの種類（`DEC-PKG-016`）。
#[must_use]
pub fn filesystem_kind(path: &Path) -> FsKind {
    fs_kind::filesystem_kind(path)
}

/// macOS の `statfs` が返す `f_fstypename` から種類を決める。
///
/// OS に依らない純粋な写像として切り出す。 文字列だけで単体試験できる。
#[must_use]
pub fn fskind_from_macos_name(name: &str) -> FsKind {
    match name {
        "apfs" => FsKind::Apfs,
        "hfs" => FsKind::Hfs,
        "msdos" | "exfat" => FsKind::Fat,
        "smbfs" | "nfs" | "afpfs" | "webdav" => FsKind::Network,
        _ => FsKind::Unknown,
    }
}

/// Linux の `statfs` が返す `f_type`（magic、下位32bit）から種類を決める。
///
/// 値は Linux の `magic.h` から取った。 `f_type` の実体の幅は libc の実装
/// （glibc / musl）ごとに違うので、呼び出し側で下位32bit へ切り詰めてから渡す。
#[must_use]
pub const fn fskind_from_linux_magic(magic: u32) -> FsKind {
    match magic {
        0xEF53 => FsKind::Ext4,
        0x9123_683E => FsKind::Btrfs,
        0x5846_5342 => FsKind::Xfs,
        0x4d44 | 0x2011_BAB0 => FsKind::Fat,
        // NFS、古い smbfs、CIFS（現行の smb1/2/3 クライアント）。
        0x6969 | 0x517B | 0xFF53_4D42 => FsKind::Network,
        _ => FsKind::Unknown,
    }
}

/// Windows の `GetVolumeInformationW` が返すファイルシステム名から種類を決める。
///
/// `ReFS` は `DEC-PKG-016` が約束した3つ（APFS / NTFS / ext4 系）に入っていない
/// ので、約束の内には数えない。
#[must_use]
pub fn fskind_from_windows_name(name: &str) -> FsKind {
    match name {
        "NTFS" => FsKind::Ntfs,
        "FAT32" | "exFAT" => FsKind::Fat,
        _ => FsKind::Unknown,
    }
}

#[cfg(target_os = "macos")]
mod fs_kind {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    use super::FsKind;

    pub(super) fn filesystem_kind(path: &Path) -> FsKind {
        let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
            return FsKind::Unknown;
        };
        // SAFETY: すべてゼロで初期化された `statfs` は妥当な初期状態。
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` は NUL 終端の有効なパス、`st` は書き込み先として渡す。
        let rc = unsafe { libc::statfs(c.as_ptr(), &raw mut st) };
        if rc != 0 {
            return FsKind::Unknown;
        }
        // SAFETY: `f_fstypename` は OS が返す NUL 終端の C 文字列。
        let name = unsafe { std::ffi::CStr::from_ptr(st.f_fstypename.as_ptr()) };
        super::fskind_from_macos_name(name.to_str().unwrap_or(""))
    }
}

#[cfg(target_os = "linux")]
mod fs_kind {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt as _;
    use std::path::Path;

    use super::FsKind;

    pub(super) fn filesystem_kind(path: &Path) -> FsKind {
        let Ok(c) = CString::new(path.as_os_str().as_bytes()) else {
            return FsKind::Unknown;
        };
        // SAFETY: すべてゼロで初期化された `statfs` は妥当な初期状態。
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c` は NUL 終端の有効なパス、`st` は書き込み先として渡す。
        let rc = unsafe { libc::statfs(c.as_ptr(), &raw mut st) };
        if rc != 0 {
            return FsKind::Unknown;
        }
        // `f_type` の型は glibc と musl で違う（`i64` のことも `i32` のことも
        // ある）。下位32bit のビットパターンだけを見れば、`magic.h` の値と
        // 一致する範囲では両方で同じ結果になる。
        #[allow(
            clippy::cast_sign_loss,
            clippy::cast_possible_truncation,
            reason = "f_type の型が libc の実装ごとに違うので、下位32bit のビットパターンで比較する"
        )]
        let magic = st.f_type as i64 as u32;
        super::fskind_from_linux_magic(magic)
    }
}

#[cfg(windows)]
mod fs_kind {
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use windows_sys::Win32::Storage::FileSystem::{
        DRIVE_REMOTE, GetDriveTypeW, GetVolumeInformationW, GetVolumePathNameW,
    };

    use super::FsKind;

    /// `MAX_PATH` ぶんの余裕を見た固定長。 ボリュームのルートパスはこれで足りる。
    const BUF_LEN: usize = 261;

    fn to_wide(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    pub(super) fn filesystem_kind(path: &Path) -> FsKind {
        let wide = to_wide(path);
        let mut volume_path = [0_u16; BUF_LEN];
        // SAFETY: `wide` は NUL 終端の文字列。`volume_path` は書き込み先で、
        // 長さをそのまま渡している。
        let ok = unsafe {
            GetVolumePathNameW(
                wide.as_ptr(),
                volume_path.as_mut_ptr(),
                u32::try_from(volume_path.len()).unwrap_or(0),
            )
        };
        if ok == 0 {
            return FsKind::Unknown;
        }

        // SAFETY: `volume_path` は直前の呼び出しが NUL 終端で埋めている。
        let drive_type = unsafe { GetDriveTypeW(volume_path.as_ptr()) };
        if drive_type == DRIVE_REMOTE {
            // ドライブ文字を NTFS 越しに見せる社内 SMB 共有もある。
            // 名前より先に「ネットワーク越しか」を確かめる。
            return FsKind::Network;
        }

        let mut fs_name = [0_u16; 32];
        // SAFETY: 出力先バッファはすべて有効な長さで渡している。
        // ボリューム名・直列番号・最大コンポーネント長・フラグは使わないので
        // 受け取らない（null を渡すと Win32 API 側が書き込みを省く）。
        let ok = unsafe {
            GetVolumeInformationW(
                volume_path.as_ptr(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                fs_name.as_mut_ptr(),
                u32::try_from(fs_name.len()).unwrap_or(0),
            )
        };
        if ok == 0 {
            return FsKind::Unknown;
        }

        let end = fs_name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(fs_name.len());
        super::fskind_from_windows_name(&String::from_utf16_lossy(&fs_name[..end]))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
mod fs_kind {
    use std::path::Path;

    use super::FsKind;

    pub(super) fn filesystem_kind(_path: &Path) -> FsKind {
        FsKind::Unknown
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

    /// ぴったりで許可しない。 実データより多めに見積もる。
    #[test]
    fn 見積もりは実データより余裕を持つ() {
        // 1行 = 12秒 × 48kHz × 4バイト = 約 2.3MB。
        let bare = 12 * 48_000 * 4;
        assert!(required_bytes(1, 48_000) > bare, "余裕が載ること");
    }

    #[test]
    fn 残量から録れる件数を出す() {
        let per_row = required_bytes(1, 48_000);
        assert_eq!(rows_that_fit(per_row * 10, 48_000), 10);
        assert_eq!(rows_that_fit(per_row - 1, 48_000), 0, "1行も入らない");
        assert_eq!(rows_that_fit(0, 48_000), 0);
    }

    #[test]
    fn 残量を引ける() {
        let got = available_bytes(&std::env::temp_dir());
        #[cfg(unix)]
        assert!(got.is_some_and(|b| b > 0), "unix では引けること");
        #[cfg(not(unix))]
        let _ = got;
    }

    /// 引けないことと 0 を混同しない。
    #[test]
    fn 引けないパスは無しを返す() {
        assert_eq!(available_bytes(Path::new("/存在しない/場所/x")), None);
    }

    #[test]
    fn 約束の内は6つだけ() {
        let promised = [
            FsKind::Apfs,
            FsKind::Hfs,
            FsKind::Ntfs,
            FsKind::Ext4,
            FsKind::Btrfs,
            FsKind::Xfs,
        ];
        for k in promised {
            assert!(k.is_promised(), "{} は約束の内のはず", k.as_str());
        }
        for k in [FsKind::Fat, FsKind::Network, FsKind::Unknown] {
            assert!(!k.is_promised(), "{} は約束の外のはず", k.as_str());
        }
    }

    #[test]
    fn macos_の名前の写し() {
        assert_eq!(fskind_from_macos_name("apfs"), FsKind::Apfs);
        assert_eq!(fskind_from_macos_name("hfs"), FsKind::Hfs);
        assert_eq!(fskind_from_macos_name("msdos"), FsKind::Fat);
        assert_eq!(fskind_from_macos_name("exfat"), FsKind::Fat);
        assert_eq!(fskind_from_macos_name("smbfs"), FsKind::Network);
        assert_eq!(fskind_from_macos_name("nfs"), FsKind::Network);
        assert_eq!(fskind_from_macos_name("afpfs"), FsKind::Network);
        assert_eq!(fskind_from_macos_name("webdav"), FsKind::Network);
        assert_eq!(
            fskind_from_macos_name("いつか増える謎の値"),
            FsKind::Unknown
        );
    }

    #[test]
    fn linux_の_magic_の写し() {
        assert_eq!(fskind_from_linux_magic(0xEF53), FsKind::Ext4);
        assert_eq!(fskind_from_linux_magic(0x9123_683E), FsKind::Btrfs);
        assert_eq!(fskind_from_linux_magic(0x5846_5342), FsKind::Xfs);
        assert_eq!(fskind_from_linux_magic(0x4d44), FsKind::Fat);
        assert_eq!(fskind_from_linux_magic(0x2011_BAB0), FsKind::Fat);
        assert_eq!(fskind_from_linux_magic(0x6969), FsKind::Network);
        assert_eq!(fskind_from_linux_magic(0x517B), FsKind::Network);
        assert_eq!(fskind_from_linux_magic(0xFF53_4D42), FsKind::Network);
        assert_eq!(
            fskind_from_linux_magic(0x6572_7546),
            FsKind::Unknown,
            "FUSE は分からない扱い"
        );
    }

    #[test]
    fn windows_の名前の写し() {
        assert_eq!(fskind_from_windows_name("NTFS"), FsKind::Ntfs);
        assert_eq!(fskind_from_windows_name("FAT32"), FsKind::Fat);
        assert_eq!(fskind_from_windows_name("exFAT"), FsKind::Fat);
        // `DEC-PKG-016` が名指しで約束したのは APFS / NTFS / ext4 系だけ。
        // ReFS は約束の内に数えない。
        assert_eq!(fskind_from_windows_name("ReFS"), FsKind::Unknown);
    }

    /// 手元の一時ディレクトリが、この OS で「約束の内」に入るファイルシステムに
    /// あることを確かめる。 CI の macOS runner は APFS、Linux runner は ext4 系の
    /// はず。分からない扱いになる環境があれば、値をそのまま記録して報告する
    /// （見なかったことにして試験から除外しない）。
    #[test]
    fn 手元の一時ディレクトリのファイルシステムを見る() {
        let kind = filesystem_kind(&std::env::temp_dir());
        // 出力は `println!` 系を禁じているので残さない（`AGENTS.md` #2）。
        // 分からない扱いになった環境があれば、この assert のメッセージで実測値を残す。
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(
            kind.is_promised(),
            "macOS / Linux の CI では約束の内のはず。実測: {kind:?}"
        );
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let _ = kind;
    }
}
