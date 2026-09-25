//! KOERU の意味の kernel（`DEC-PLT-034`）。
//!
//! 同じ入力から同じ判断を作るものだけを置く。 素材と要求の解決（綴り・収録単位・
//! 方式）、録音リストの組み立て、曲と被覆、編集と確認の規則。
//!
//! 入出力を持たない。 SQLite も、ファイルも、画面も、OS の音声 API も知らない。
//! M6 では同じ規則を WASM から呼ぶ（編集のドラッグ中の予測）ので、ここに置いた規則は
//! native と WASM で同じ答えを返す。 依存の許可は `tests/purity.rs` が見る。
//!
//! 移行中。 `koeru-core` が同じ名前で再輸出しているので、既存の `koeru_core::alias` などの
//! 経路はそのまま通る。

pub mod alias;
pub mod calibration;
pub mod channel;
pub mod confidence;
pub mod coverage;
pub mod downgrade;
pub mod guide;
pub mod id;
pub mod inventory;
pub mod leak;
pub mod mora;
pub mod names;
pub mod order;
pub mod oto;
pub mod pace;
pub mod plan;
pub mod presamp;
pub mod preset;
pub mod reach;
pub mod reclist;
pub mod retake;
pub mod review;
pub mod selection;
pub mod song;
pub mod target;
pub mod text;
pub mod tone;
pub mod validate;
pub mod voice;
pub mod waveform;
