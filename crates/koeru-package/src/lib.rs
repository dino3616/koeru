//! 配布パッケージの生成（`PROFILE-M4`）。
//!
//! 入口は [`tree::build`]（音源ルートを組み立てる）、[`validate::validate`]
//! （書き出し前検証）、[`archive::write`]（ZIP と UAR）、
//! [`archive::verify`]（読み戻し検証）の4つ。
//!
//! 形式的な契約は `specs/requirements/packaging-export.fsl` が持つ。
//! 検証 → ZIP → 読み戻しの順序と、失敗したら ZIP を残さないことは
//! そちらが正本で、ここはその実装。
//!
//! # 何をしないか
//!
//! 下位方式への書き出しは持たない（`PROFILE-M4` の excludes）。
//! [`coverage`] が「どの方式なら出せるか」を答えるところまでで、
//! oto の5値を再導出する経路は無い。再導出は `koeru-align` が持つ（`TR-ALN-34`）。
//!
//! 配布の場も署名も持たない（`DEC-PKG-003`）。ここが作るのは
//! 「渡せる状態のファイル」まで。

/// 被覆と下位方式への書き出しの計画は、書き出しの可否を決める規則なので
/// `koeru-model` へ移した（`DEC-PLT-034`）。 既存の経路を通すための再輸出。 **移行中。**
pub use koeru_model::{coverage, downgrade};

pub mod archive;
pub mod bank;
pub mod character;
pub mod icon;
pub mod profile;
pub mod readme;
/// 音階の名前（`TR-PKG-04`, `TR-PKG-29`）。
///
/// 実体は `koeru-core` にある。 録音側も音高ごとのディレクトリ名に使うので
/// （`TR-REC-36`）、配布側だけが持つものではなくなった。
pub use koeru_core::tone;
pub mod tree;
pub mod validate;
