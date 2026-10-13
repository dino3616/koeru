//! 配布パッケージの生成（`PROFILE-M4`）。
//!
//! 入口は [`tree::build`]（音源ルートを組み立てる）、[`validate::validate`]
//! （書き出し前検証）、[`archive::expect`]（読み戻しが突き合わせるもの）の3つ。
//!
//! 方針だけを持ち、ファイルは開かない（`tests/purity.rs`）。 素材の事実
//! （[`validate::MasterFact`]）は呼び出し側が先に調べて渡し、ZIP の符号化と読み戻しは
//! `koeru-formats` の [`koeru_formats::archive`]、WAV の符号化は
//! [`koeru_formats::wav`] が持つ。 書いて、読み戻して、通らなければ消す、までの
//! ファイルの手順は呼び出し側（`koeru-app` の `export_io`）。
//!
//! 形式的な契約は `specs/requirements/packaging-export.fsl` が持つ。
//! 検証 → ZIP → 読み戻しの順序と、失敗したら ZIP を残さないことは
//! そちらが正本で、ここはその方針の部分。
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
/// 実体は `koeru-model` にある。 録音側も音高ごとのディレクトリ名に使うので
/// （`TR-REC-36`）、配布側だけが持つものではなくなった。
pub use koeru_model::tone;
pub mod tree;
pub mod validate;
