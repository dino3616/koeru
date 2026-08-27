//! トレースの初期化。
//!
//! 出力は `tracing` に統一する。`println!` / `eprintln!` / `dbg!` は lint で禁止している。
//!
//! **段は3つに分ける。**
//!
//! 1. `fmt` 層 — 開発時の人間向け出力。`RUST_LOG` で制御する。
//! 2. ファイル層 — 利用者の端末にローカル保存する。障害報告に添付できる。
//! 3. 送信層 — オプトインのときだけ有効化する。**ここに渡す前にフィールドを絞る。**
//!
//! 送信層は既定で無効。有効化してもホワイトリストに載ったフィールドしか通さない。
//! 音源名・ファイルパス・歌詞・プロジェクト名は通さない。

use std::fmt;

/// トレースの送信可否。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Consent {
    /// 既定。ローカルに留める。
    #[default]
    LocalOnly,
    /// 利用者が明示的に許可した。
    OptedIn,
}

/// 初期化の設定。
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// `RUST_LOG` 相当のフィルタ。未指定なら `info` 相当を使う。
    pub filter: Option<String>,
    /// 送信可否。既定は `Consent::LocalOnly`。
    pub consent: Consent,
}

/// 初期化に失敗した理由。
///
/// ブートストラップ層で使うので、呼び出し側は `.expect("トレースの初期化")` でよい。
/// ここで回復する意味はなく、失敗したら即座に気づけるほうが良い。
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("フィルタ指定を解釈できない: {spec}")]
    InvalidFilter { spec: String },

    #[error("トレースの初期化が二重に呼ばれた")]
    AlreadyInitialized,
}

/// 送信層に渡してよいフィールド名のホワイトリスト。
///
/// **ここに載っていないフィールドは送信層へ渡さない。** ブラックリスト方式にすると必ず漏れる。
pub const SENDABLE_FIELDS: &[&str] = &[
    "error.kind",     // Error::telemetry_kind の戻り値
    "phase",          // 録音 / 試唱 / 書き出し などの工程名
    "method",         // 単独音 / 連続音 / CVVC / 多音階
    "screen",         // 画面の識別子
    "outcome",        // 成功 / 失敗 / 中断
    "elapsed_ms",     // 所要時間
    "take_index",     // 何件目のテイクか（内容は含まない）
    "coverage_ratio", // カバレッジの割合
    "app.version",
    "os.name",
];

/// フィールド名が送信可能かを判定する。
#[must_use]
pub fn is_sendable(field: &str) -> bool {
    SENDABLE_FIELDS.contains(&field)
}

/// 人間向けの短い説明。
impl fmt::Display for Consent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LocalOnly => f.write_str("ローカルのみ"),
            Self::OptedIn => f.write_str("送信を許可"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定は送信しない() {
        assert_eq!(Config::default().consent, Consent::LocalOnly);
    }

    #[test]
    fn 音源名は送信できない() {
        assert!(!is_sendable("voicebank.name"));
        assert!(!is_sendable("project.path"));
        assert!(!is_sendable("lyrics"));
        assert!(is_sendable("error.kind"));
    }
}
