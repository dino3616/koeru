//! diesel のスキーマ定義。
//!
//! **`migrations/` の SQL と対応させて手で書く。** `diesel print-schema` は
//! CLI の導入が要るので、生成物をリポジトリへ置くのではなく、
//! **マイグレーションとこのファイルの両方をレビュー対象にする。**
//! 食い違いは [`crate::db`] のテストが実際に SQL を投げて検出する。

// diesel のマクロが生成するコードは、この lint の対象から外す。
#![allow(
    clippy::pub_underscore_fields,
    clippy::ref_option,
    missing_debug_implementations
)]

diesel::table! {
    /// 収録セッション。録音条件のスナップショット（`TR-REC-30`）。
    sessions (id) {
        id -> Integer,
        started_at -> Text,
        device_id -> Text,
        sample_rate_hz -> Integer,
        channels -> Integer,
        effects_state -> Text,
        route -> Text,
    }
}

diesel::table! {
    /// 録音リストの行（`TR-RCL-18`）。
    rows (id) {
        id -> Text,
        text -> Text,
        file_stem -> Text,
        tone -> Integer,
        state -> Text,
        ordinal -> Integer,
    }
}

diesel::table! {
    /// 行が生む収録単位。**カバレッジはここから導出する。**
    row_units (row_id, kana) {
        row_id -> Text,
        kana -> Text,
        consonant -> Text,
        vowel -> Text,
    }
}

diesel::table! {
    /// テイク。**世代として積む**（`TR-REC-21`）。
    takes (id) {
        id -> Integer,
        row_id -> Text,
        session_id -> Integer,
        rel_path -> Text,
        frames -> BigInt,
        recorded_at -> Text,
        invalid -> Integer,
        generation -> Integer,
    }
}

diesel::table! {
    /// 採用テイク。行ごとに高々1つ。
    adopted_takes (row_id) {
        row_id -> Text,
        take_id -> Integer,
    }
}

diesel::table! {
    /// oto の5値と確認状態。
    oto_values (take_id) {
        take_id -> Integer,
        offset_ms -> Double,
        consonant_ms -> Double,
        cutoff_ms -> Double,
        preutterance_ms -> Double,
        overlap_ms -> Double,
        confidence -> Double,
        confirmed -> Integer,
        hand_edited -> Integer,
    }
}

diesel::joinable!(row_units -> rows (row_id));
diesel::joinable!(takes -> rows (row_id));
diesel::joinable!(takes -> sessions (session_id));
diesel::joinable!(adopted_takes -> rows (row_id));
diesel::joinable!(adopted_takes -> takes (take_id));
diesel::joinable!(oto_values -> takes (take_id));

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    rows,
    row_units,
    takes,
    adopted_takes,
    oto_values,
);
