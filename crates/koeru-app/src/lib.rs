//! KOERU のアプリケーション層。
//!
//! ドメイン層と GUI の間の細い層。 判断はここに置かない。
//!
//! 単一のアプリケーションとして完結する（`TR-PLT-21`, `TR-PLT-07`）。
//! 常駐する別プロセスも、外部のサービスも持たない。
//! スクリプト言語ランタイムを配布物に含めない（`TR-PLT-07`）——
//! それを `tests/offline.rs` が検査する。
//!
//! GUI の基盤は Tauri（`TR-PLT-03`, `DEC-PLT-015`）。
//! vLabeler / RecStar からは部品を取らない（`TR-PLT-11`）。
//! [`studio`] が筋を組み立て、[`commands`] は Tauri へ渡すだけ。

pub mod align;
pub mod commands;
pub mod error;
pub mod external;
pub mod latency;
pub mod packaging;
pub mod preview;
pub mod pump;
pub mod review;
pub mod storage;
pub mod studio;
pub mod workers;

pub use error::{AppError, Result};
pub use studio::Studio;

/// 起動時にライブラリの置き場所について分かったこと（`DEC-PKG-016`）。
///
/// [`Studio`] が保持し、[`Studio::boot`] で読める。 画面への表示はまだ持たない
/// （表示は T09 の notices）。ここは検出と結果の保持まで。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LibraryBoot {
    /// ライブラリを置いたファイルシステムの種類。
    pub fs_kind: storage::FsKind,
    /// Roaming から Local への移し替えの結果。
    ///
    /// macOS と Linux は Roaming と Local を区別しないので、移し替えを試みない
    /// （`None`）。Windows は移し替えを試みる。呼べて値が返れば `Some`、
    /// 呼び出し自体が失敗したら `None`——その場合は `old`（Roaming）を開いて
    /// 起動を続ける（`crate::run` の「解釈で決めたもの」）。
    pub relocation: Option<koeru_core::relocate::Relocation>,
}

/// 画面へ渡すコマンドの一覧。
///
/// `tauri::generate_handler!` ではなくこちらを通す（`DEC-PLT-019`）。
/// ここが TS 側の呼び出し口と型の正本になり、`bindings.gen.ts` が生成される。
/// 手で両方に足すのをやめるための層なので、コマンドの追加はここだけに書く。
#[must_use]
pub fn builder() -> tauri_specta::Builder<tauri::Wry> {
    tauri_specta::Builder::<tauri::Wry>::new().commands(tauri_specta::collect_commands![
        commands::list_devices,
        commands::list_projects,
        commands::method_presets,
        commands::tone_suggestions,
        commands::tone_options,
        commands::create_project,
        commands::rename_project,
        commands::voice_state,
        commands::open_project,
        commands::presamp_notice,
        commands::dismiss_presamp_notice,
        commands::progress,
        commands::chosen_device,
        commands::arm_device,
        commands::probe_input,
        commands::start_take,
        commands::start_retake,
        commands::rows_with_takes,
        commands::recording_order,
        commands::set_recording_order,
        commands::adopt_take,
        commands::otos_of_take,
        commands::play_take,
        commands::stream_envelope,
        commands::stop_envelope_stream,
        commands::finish_take,
        commands::preview,
        commands::preroll_ms,
        commands::estimate_space,
        commands::calibrate,
        commands::gain_drift,
        commands::restore_saved_gain,
        commands::auto_advance_ms,
        commands::output_kind,
        commands::check_guide_leak,
        commands::play_pitch,
        commands::song_status,
        commands::song_plan,
        commands::song_file_preview,
        commands::import_songs,
        commands::rename_song,
        commands::all_songs,
        commands::set_song_transpose,
        commands::set_song_in_bank,
        commands::repack_for_selection,
        commands::song_notes,
        commands::sing_song,
        commands::pending_work,
        commands::latency_report,
        commands::waveform_window,
        commands::spectrogram_window,
        commands::preflight,
        commands::use_mixed_channels,
        commands::stop_preview,
        commands::review_summary,
        commands::review_queue,
        commands::confirm_entry,
        commands::confirm_all_entries,
        commands::switch_review_mode,
        commands::edit_oto_value,
        commands::revert_oto_value,
        commands::rerecord_entry,
        commands::validate_otos,
        commands::export_otos,
        commands::stale_takes,
        commands::model_notice,
        commands::package_settings,
        commands::set_package_settings,
        commands::set_package_icon,
        commands::set_package_portrait,
        commands::package_icon,
        commands::package_portrait,
        commands::package_state,
        commands::package_contents,
        commands::export_package,
        commands::export_downgrade,
        commands::releases,
        commands::reveal_release,
    ])
}

/// アプリを起動する。
///
/// ライブラリはアプリ管理のデータディレクトリ配下に置く（`TR-PKG-37`）。
/// 利用者に保存先を選ばせない（`TR-PKG-45`）。
///
/// **Windows だけ、既定の置き場所が Roaming から Local へ変わる**（`DEC-PKG-016`）。
/// macOS と Linux は Tauri の `app_data_dir` と `app_local_data_dir` が同じパスを
/// 指す（Tauri の実装で確かめた）ので、この分岐に入らない。
///
/// # Panics
///
/// ブートストラップに失敗したら落ちる。ここは回復する意味が無い層。
pub fn run() {
    // 出力は tracing に統一する。 println! は lint で禁じている。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("koeru=info,warn")),
        )
        .init();

    let specta_builder = builder();

    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager as _;
            let old = app
                .path()
                .app_data_dir()
                .expect("アプリのデータディレクトリを取れること")
                .join("library");
            let new = app
                .path()
                .app_local_data_dir()
                .expect("アプリのローカルデータディレクトリを取れること")
                .join("library");

            // `old == new` なら macOS / Linux。 移し替えを試みる意味が無い
            // （壊すものが無い代わりに、直せるものも無い）。
            let (root, relocation) = if old == new {
                (new, None)
            } else {
                match koeru_core::relocate::relocate_library(&old, &new) {
                    Ok(r) => (new, Some(r)),
                    Err(e) => {
                        // 移し替えの失敗で起動を止めない。 `old`（Roaming）を
                        // 開いて続ける——暫定の解釈（「解釈で決めたもの」参照）。
                        // 詳細な原因はここでは畳まない。code と分類だけを記録する。
                        koeru_failure::record_failure(
                            &e,
                            koeru_failure::Outcome::NotStarted,
                            "library_relocate",
                        );
                        (old, None)
                    }
                }
            };

            let fs_kind = storage::filesystem_kind(&root);
            if !fs_kind.is_promised() {
                // 起動時に、約束の外にいることを知らせる（`DEC-PKG-016`）。
                // 画面への表示はまだ無い（T09 の notices）。
                tracing::warn!(
                    fs_kind = fs_kind.as_str(),
                    "ライブラリがネットワーク上か FAT 系のファイルシステムにある"
                );
            }
            if let Some(r) = relocation {
                tracing::info!(
                    relocation = r.as_str(),
                    "ライブラリの置き場所の移し替えを確かめた"
                );
            }

            let mut studio = Studio::open(root).expect("ライブラリを開けること");
            studio.boot = LibraryBoot {
                fs_kind,
                relocation,
            };
            app.manage(commands::AppState::new(studio));
            Ok(())
        })
        .invoke_handler(specta_builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("Tauri を起動できること");
}
