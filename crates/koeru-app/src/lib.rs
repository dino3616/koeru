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
pub mod preview;
pub mod pump;
pub mod storage;
pub mod studio;
pub mod workers;

pub use error::{AppError, Result};
pub use studio::Studio;

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
        commands::create_project,
        commands::open_project,
        commands::progress,
        commands::arm_device,
        commands::probe_input,
        commands::start_take,
        commands::start_retake,
        commands::rows_with_takes,
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
        commands::import_ust,
        commands::set_song_in_bank,
        commands::sing_song,
        commands::pending_work,
        commands::latency_report,
        commands::waveform_window,
        commands::spectrogram_window,
        commands::preflight,
        commands::use_mixed_channels,
        commands::stop_preview,
    ])
}

/// アプリを起動する。
///
/// ライブラリはアプリ管理のデータディレクトリ配下に置く（`TR-PKG-37`）。
/// 利用者に保存先を選ばせない（`TR-PKG-45`）。
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
            let root = app
                .path()
                .app_data_dir()
                .expect("アプリのデータディレクトリを取れること")
                .join("library");
            let studio = Studio::open(root).expect("ライブラリを開けること");
            app.manage(commands::AppState::new(studio));
            Ok(())
        })
        .invoke_handler(specta_builder.invoke_handler())
        .run(tauri::generate_context!())
        .expect("Tauri を起動できること");
}
