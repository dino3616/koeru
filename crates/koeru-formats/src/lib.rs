//! KOERU が読み書きする外部形式（`DEC-PLT-034`）。
//!
//! 構文と符号化だけを置く。 `oto.ini` の行、周波数表（`.frq`）のバイト列、
//! CP932 / UTF-8 の読み書き、WAV と ZIP の符号化、UST / USTX（曲）の構文。 どれを採用するか、
//! 台帳のどれと対応するか、確信度をどう扱うかは持たない。
//!
//! ファイルを開かない。 受け取るのはバイト列と文字列、または `Read` / `Write` /
//! `Seek` で、ファイルを開いて渡すのは呼び出し側。 依存とソースの純度は
//! `tests/purity.rs` が見る。
//!
//! 移行中。 `koeru_core::text` / `koeru_core::frq` / `koeru_core::ust` /
//! `koeru_align::ini` / `koeru_audio::wav` が同じものを再輸出しているので、既存の経路は
//! そのまま通る。

pub mod archive;
pub mod frq;
pub mod oto_ini;
pub mod text;
pub mod ust;
pub mod wav;
