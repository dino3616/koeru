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
pub mod capture_lease;
pub mod commands;
pub mod error;
pub mod external;
pub mod latency;
pub mod packaging;
pub mod playback_lease;
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

/// デバイスの見張りスレッドが、一覧の変化を確かめに行く間隔（ミリ秒、`TR-REC-04`）。
///
/// RT コールバックではないので、多少の遅れは構わない。 短すぎると起こさなくてよい
/// スレッドを起こし続け、長すぎると消失に気づくのが遅れる。
const DEVICE_WATCH_POLL_MS: u64 = 220;

/// デバイス一覧が変わったときだけ `Studio::check_device` を呼ぶ（`TR-REC-04`）。
///
/// `finish_take` が `AppState` の `studio` を数秒握ることがあるので、
/// 変化が無い間は毎周期そこを取りに行かない——読むのはカウンタのアトミックな
/// load 1回だけ（`commands::AppState::device_watch_count`）。
///
/// `stop` が立ったら抜ける。 アプリの終了（`RunEvent::Exit`）で立てる。
fn watch_devices(handle: &tauri::AppHandle, stop: &std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    use tauri::Manager as _;

    let mut last = 0_usize;
    while !stop.load(Ordering::Acquire) {
        std::thread::sleep(std::time::Duration::from_millis(DEVICE_WATCH_POLL_MS));
        if stop.load(Ordering::Acquire) {
            break;
        }
        let Some(state) = handle.try_state::<commands::AppState>() else {
            continue;
        };
        let Some(count) = state.device_watch_count() else {
            continue; // まだ一度もデバイスを開いていない。
        };
        if count == last {
            continue;
        }
        last = count;
        match state.lock_studio() {
            Ok(mut studio) => {
                if let Err(e) = studio.check_device() {
                    // 自動の見張りが断られただけ。 画面へは渡さず、その場で記録する
                    // （持ち主がその場で決めて縮退する経路。`DEC-PLT-038`）。
                    e.record("device_watch.check");
                }
            }
            Err(e) => e.record("device_watch.check"),
        }
    }
}

/// アプリを起動する。
///
/// ライブラリはアプリ管理のデータディレクトリ配下に置く（`TR-PKG-37`）。
/// 利用者に保存先を選ばせない（`TR-PKG-45`）。
///
/// 置き場所は `app_local_data_dir`（`DEC-PKG-016`）。 Windows で `app_data_dir` は
/// Roaming を指し、ネットワーク上に置かれうる。 macOS と Linux は2つが同じパスを指す。
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

    // デバイスの見張りスレッドへの停止合図（`TR-REC-04`）。 アプリの終了で立てる。
    let device_watch_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));

    let app = tauri::Builder::default()
        .setup({
            let stop = std::sync::Arc::clone(&device_watch_stop);
            move |app| {
                use tauri::Manager as _;
                let root = app
                    .path()
                    .app_local_data_dir()
                    .expect("アプリのローカルデータディレクトリを取れること")
                    .join("library");

                let fs_kind = storage::filesystem_kind(&root);
                if !fs_kind.is_promised() {
                    // 起動時に、約束の外にいることを知らせる（`DEC-PKG-016`）。
                    // 画面への表示はまだ無い（T09 の notices）。
                    tracing::warn!(
                        fs_kind = fs_kind.as_str(),
                        "ライブラリがネットワーク上か FAT 系のファイルシステムにある"
                    );
                }
                let mut studio = Studio::open(root).expect("ライブラリを開けること");
                studio.boot = LibraryBoot { fs_kind };
                app.manage(commands::AppState::new(studio));

                // デバイスの着脱を見張る（`TR-REC-04`）。 `AppHandle` は
                // `Clone` + `'static` で、スレッドから `try_state` で
                // 引き直せる——`Studio` を直に持ち回さない。
                let handle = app.handle().clone();
                std::thread::spawn(move || watch_devices(&handle, &stop));
                Ok(())
            }
        })
        .invoke_handler(specta_builder.invoke_handler())
        .build(tauri::generate_context!())
        .expect("Tauri を起動できること");

    app.run(move |_handle, event| {
        if matches!(event, tauri::RunEvent::Exit) {
            device_watch_stop.store(true, std::sync::atomic::Ordering::Release);
        }
    });
}
