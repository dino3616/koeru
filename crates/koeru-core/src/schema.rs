//! diesel のスキーマ定義。
//!
//! `migrations/` の SQL と対応させて手で書く。 `diesel print-schema` は
//! CLI の導入が要るので、生成物をリポジトリへ置くのではなく、
//! マイグレーションとこのファイルの両方をレビュー対象にする。
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
        source_channel -> Integer,
        master_rate_hz -> Integer,
        resampler -> Text,
        upstream_conversion -> Text,
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
    /// 行が生む収録単位。カバレッジはここから導出する。
    row_units (row_id, kana) {
        row_id -> Text,
        kana -> Text,
        consonant -> Text,
        vowel -> Text,
    }
}

diesel::table! {
    /// テイク。世代として積む（`TR-REC-21`）。
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
    oto_values (take_id, alias) {
        take_id -> Integer,
        alias -> Text,
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

diesel::table! {
    /// 録音停止時に算出した解析値。書き出しと再開で WAV を再走査しない
    /// （`TR-PKG-05`, `TR-PKG-42`）。
    take_analysis (take_id) {
        take_id -> Integer,
        peak -> Double,
        hop_size -> Integer,
        f0 -> Binary,
        amp -> Binary,
        thumbnail -> Binary,
    }
}

diesel::table! {
    /// 書き出し単位のリリースレコード（`TR-PKG-44`）。不変。
    releases (seq) {
        seq -> Integer,
        version -> Text,
        method -> Text,
        alias_count -> Integer,
        validation -> Text,
        oto_hash -> Text,
        terms_hash -> Text,
        archive_name -> Text,
        released_at -> Text,
    }
}

diesel::table! {
    /// テイクごとの計測値（`TR-REC-16`）と取りこぼし（`TR-REC-07`）。
    take_metrics (take_id) {
        take_id -> Integer,
        peak_dbfs -> Double,
        rms -> Double,
        full_scale_runs -> Integer,
        dc_offset -> Double,
        noise_floor_rms -> Double,
        leading_margin_ms -> Double,
        trailing_margin_ms -> Double,
        discontinuities -> Integer,
        preroll_frames -> Integer,
        guide_offset_frames -> Nullable<BigInt>,
    }
}

diesel::table! {
    /// 入力レベルの校正（`TR-REC-14`, `TR-REC-15`）。デバイスごとに1つ。
    calibrations (device_id) {
        device_id -> Text,
        gain -> Nullable<Float>,
        control -> Text,
        peak_dbfs -> Double,
        settled -> Integer,
        measured_at -> Text,
        source_channel -> Integer,
    }
}

diesel::table! {
    /// 課題曲（`TR-RCL-12`）。曲バンクを持たない。
    songs (id) {
        id -> Text,
        title -> Text,
        source -> Text,
        license -> Text,
        bundled -> Integer,
        in_bank -> Integer,
        added_at -> Text,
    }
}

diesel::table! {
    /// 曲のノート（`TR-RCL-12` (a)(b)）。
    song_notes (song_id, ordinal) {
        song_id -> Text,
        ordinal -> Integer,
        lyric -> Text,
        midi -> Integer,
        ticks -> Integer,
    }
}

diesel::joinable!(song_notes -> songs (song_id));

diesel::joinable!(row_units -> rows (row_id));
diesel::joinable!(takes -> rows (row_id));
diesel::joinable!(takes -> sessions (session_id));
diesel::joinable!(adopted_takes -> rows (row_id));
diesel::joinable!(adopted_takes -> takes (take_id));
diesel::joinable!(oto_values -> takes (take_id));
diesel::joinable!(take_analysis -> takes (take_id));
diesel::joinable!(take_metrics -> takes (take_id));

diesel::allow_tables_to_appear_in_same_query!(
    sessions,
    rows,
    row_units,
    takes,
    adopted_takes,
    oto_values,
    take_analysis,
    take_metrics,
    releases,
    calibrations,
    songs,
    song_notes,
);
