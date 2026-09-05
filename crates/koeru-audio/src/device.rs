//! 入力デバイスの同一性。
//!
//! 識別子は表示名ではない（`TR-REC-03`）。Windows はエンドポイント ID 文字列、
//! macOS は `kAudioDevicePropertyDeviceUID`、Linux は ALSA / PipeWire のノード名。
//! 表示名は本人が付けた名前を含みうるので、識別にも比較にも使わない。

/// プロジェクトに固定する、デバイスの識別子。
///
/// 復帰の判定（`same_device_returned`）はこの値の一致だけで決める。
/// 別のデバイスへ自動で切り替えないことが `REQ-REC-109` の中身。
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DeviceId(String);

impl DeviceId {
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// 識別子を丸ごとトレースへ流さない。 Linux のノード名にはユーザー名が入ることがある。
// 同一性の追跡には長さと先頭だけで足り、突き合わせは端末内のログでできる。
impl std::fmt::Debug for DeviceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let head: String = self.0.chars().take(4).collect();
        write!(f, "DeviceId({head}… len={})", self.0.len())
    }
}

/// デバイス一覧に出す情報（`TR-REC-03`）。
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub id: DeviceId,
    /// 表示名。トレースにも送信にも載せない。 本人が付けた名前を含みうる。
    pub name: RedactedName,
    pub is_default: bool,
    pub native_sample_rate_hz: u32,
    pub channels: u16,
}

/// 画面には出すが、ログには出さない文字列。
#[derive(Clone)]
pub struct RedactedName(String);

impl RedactedName {
    #[must_use]
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    /// 画面へ出すときだけ使う。
    #[must_use]
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for RedactedName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RedactedName(<伏せ>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 表示名はデバッグ出力に出ない() {
        let name = RedactedName::new("ハルの AirPods");
        assert!(!format!("{name:?}").contains("ハル"));
    }

    #[test]
    fn 識別子は先頭以外がデバッグ出力に出ない() {
        let id = DeviceId::new("alsa_input.usb-0000_haruto-00.analog-stereo");
        let shown = format!("{id:?}");
        assert!(!shown.contains("haruto"));
        assert!(shown.contains("alsa"));
    }

    #[test]
    fn 識別子は完全一致でだけ等しい() {
        assert_eq!(DeviceId::new("abc"), DeviceId::new("abc"));
        assert_ne!(DeviceId::new("abc"), DeviceId::new("abd"));
    }
}
