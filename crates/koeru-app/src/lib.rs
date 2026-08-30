//! KOERU のアプリケーション層。
//!
//! **ドメイン層と GUI の間の細い層。** 判断はここに置かない。
//! [`studio`] が筋を組み立て、[`commands`] は Tauri へ渡すだけ。

pub mod commands;
pub mod error;
pub mod pump;
pub mod storage;
pub mod studio;

pub use error::{AppError, Result};
pub use studio::Studio;

/// アプリを起動する。
///
/// **ライブラリはアプリ管理のデータディレクトリ配下に置く**（`TR-PKG-37`）。
/// 利用者に保存先を選ばせない（`TR-PKG-45`）。
///
/// # Panics
///
/// ブートストラップに失敗したら落ちる。**ここは回復する意味が無い層。**
pub fn run() {
    // **出力は tracing に統一する。** println! は lint で禁じている。
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("koeru=info,warn")),
        )
        .init();

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
        .invoke_handler(tauri::generate_handler![
            commands::list_devices,
            commands::list_projects,
            commands::create_project,
            commands::open_project,
            commands::progress,
            commands::arm_device,
            commands::probe_input,
            commands::start_take,
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
            commands::stop_preview,
        ])
        .run(tauri::generate_context!())
        .expect("Tauri を起動できること");
}
