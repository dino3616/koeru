//! 縦切りの組み立て。**録って、聴けるまでを1本に繋ぐ。**
//!
//! ここに Tauri は出てこない。**アプリの筋を、GUI 無しで検査できるようにする。**
//!
//! ## 通す順序
//!
//! 1. デバイスを選び、ストリームを開く（`recording-input.fsl` の手順どおり）
//! 2. 行を1つ録る（`.wav.part` → fsync → rename → DB コミット）
//! 3. 録音停止時に解析を確定させ、`.frq` を書く（`TR-PKG-05`）
//! 4. 境界を見つけて oto の5値を導く
//! 5. 目標音高で合成して鳴らす
//!
//! **3 と 4 を録音停止の直後に済ませるのが要点。** 後回しにすると、
//! 試唱のたびに WAV を読み直すことになる（`TR-PKG-42`）。

use std::path::PathBuf;

use koeru_audio::backend::macos as mac;
use koeru_audio::{DeviceId, Session, wav};
use koeru_core::analysis::{TakeAnalysis, TakeMetrics};
use koeru_core::calibration::{self, Calibration, Outcome};
use koeru_core::db::{FinalizedTake, Ledger, SessionSnapshot, koeru_oto};
use koeru_core::frq;
use koeru_core::guide::{self, GuideSpec};
use koeru_core::inventory::UnitSet;
use koeru_core::leak::{self, LeakCheck};
use koeru_core::project::{CoverageState, HandoffState, Library, Manifest, Method, ProjectDir};
use koeru_core::reclist::{DEFAULT_UNITS_PER_ROW, generate_single};
use koeru_core::song::{self, Song, SongStatus};
use koeru_core::ust;
use koeru_synth::oto::{Oto, OtoPreset, derive_cv};
use koeru_synth::resampler::{RenderRequest, render};
use koeru_synth::segment::{SegmentConfig, confidence, detect_single};

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::pump::{PREROLL_MS, Pump};
use crate::storage;

/// 素材の F0 を探す下限（Hz）。**歌声の音域を広く取る。**
///
/// 目標音高から範囲を作ってはいけない。素材は別の音高で録られている
/// （`koeru-synth` の resampler と同じ理由）。
const SOURCE_F0_FLOOR_HZ: f64 = 55.0;
/// 素材の F0 を探す上限（Hz）。
const SOURCE_F0_CEIL_HZ: f64 = 1100.0;

/// 単独音の収録音高。**A3 を既定にする**（`TR-RCL` の音階既定）。
pub const DEFAULT_TONE_MIDI: i32 = 57;

/// リングの容量（サンプル）。**8秒ぶん。**
///
/// 描画やディスクが詰まっても、この長さのあいだは取りこぼさない。
const RING_SECONDS: usize = 8;

/// 開いているプロジェクト。
#[derive(Debug)]
struct Open {
    dir: ProjectDir,
    ledger: Ledger,
    session_id: i32,
}

/// 1つのテイクの結果。
#[derive(Debug, Clone, PartialEq)]
pub struct TakeResult {
    /// 台帳の ID。
    pub take_id: i32,
    /// どの行か。
    pub row_id: String,
    /// 長さ（ミリ秒）。
    pub duration_ms: f64,
    /// 絶対値の最大。**1.0 に達していたらクリップ。**
    pub peak: f32,
    /// 波形サムネイル（0〜255）。
    pub thumbnail: Vec<u8>,
    /// 導けた oto。**発声が見つからなければ `None`。**
    pub oto: Option<Oto>,
    /// 境界の確信度。
    pub confidence: Option<f64>,
    /// 取りこぼしの回数（`TR-REC-07`）。
    pub discontinuities: usize,
    /// **取りこぼしたので自動的に無効にした**（`TR-REC-07`）。
    /// 同じフレーズがもう一度出てくる。
    pub invalidated: bool,
    /// 計測値（`TR-REC-16`）。**測るだけで、判定も指摘もしない。**
    pub metrics: TakeMetrics,
    /// 押した瞬間より前から何ミリ秒ぶん遡れたか（`TR-REC-19`）。
    pub preroll_ms: f64,
}

/// 残量の見積もり（`TR-REC-41`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceEstimate {
    /// まだ録っていない行の数。
    pub remaining_rows: u64,
    /// 残り全部に要るバイト数。
    pub required_bytes: u64,
    /// 保存先の空き。**引けなければ `None`。**
    pub available_bytes: Option<u64>,
    /// **その残量で録りきれる件数**（`TR-REC-41`）。
    pub rows_that_fit: u64,
}

impl SpaceEstimate {
    /// 残り全部を録りきれるか。
    #[must_use]
    pub const fn is_sufficient(&self) -> bool {
        self.rows_that_fit >= self.remaining_rows
    }
}

/// プロジェクトの現在地。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Progress {
    /// 次に録る行（`(id, 読み上げる文字列)`）。**全部録れていれば `None`。**
    pub next_row: Option<(String, String)>,
    /// 収録済み単位の数。
    pub covered: usize,
    /// 必要な単位の数。
    pub required: usize,
    /// 完成状態。
    pub coverage: CoverageState,
    /// 手渡し状態。**完成判定はこれを見ない**（`TR-PKG-33`）。
    pub handoff: HandoffState,
    /// **いま歌える曲の数**（`TR-RCL-19`）。カバレッジと常に両方出す。
    pub singable_songs: usize,
    /// バンクに入っている曲の数。**0 でも成立する。**
    pub songs_in_bank: usize,
}

/// 縦切りの本体。
#[derive(Debug)]
pub struct Studio {
    library: Library,
    open: Option<Open>,
    capture: Option<mac::Capture>,
    /// 排出スレッド。**収録画面にいる間ずっと回っている**（`TR-REC-19`）。
    pump: Option<Pump>,
    session: Session,
    /// 録音中の行。
    recording: Option<String>,
    /// 収録開始時点の取りこぼし数。**このテイクの中で増えたぶんだけを見る**（`TR-REC-07`）。
    xrun_baseline: usize,
    /// 選んでいるデバイス。
    device: Option<DeviceId>,
    /// アプリが触る前のゲイン。**終了時にここへ戻す**（`TR-REC-15`）。
    gain_before: Option<(DeviceId, f32)>,
    /// 回り込みの検査結果（`TR-REC-24`）。**済むまで音高提示を鳴らさない。**
    leak: Option<LeakCheck>,
    playback: Option<mac::Playback>,
}

impl Studio {
    /// ライブラリを開く。**無ければ作る。**
    #[tracing::instrument(skip(library_root), err)]
    pub fn open(library_root: PathBuf) -> Result<Self> {
        Ok(Self {
            library: Library::open(library_root)?,
            open: None,
            capture: None,
            pump: None,
            // 3件までの提示上限は `recording-input.fsl` と揃える。
            session: Session::new(3),
            recording: None,
            xrun_baseline: 0,
            device: None,
            gain_before: None,
            leak: None,
            playback: None,
        })
    }

    /// ライブラリの中身。**manifest が読めないものも落とさず返す。**
    #[tracing::instrument(skip(self), err)]
    pub fn projects(&self) -> Result<Vec<(Uuid, Option<Manifest>)>> {
        Ok(self
            .library
            .list()?
            .into_iter()
            .map(|(d, m)| (d.id(), m.ok()))
            .collect())
    }

    /// プロジェクトを作り、録音リストを入れる。
    ///
    /// **リスト生成まで一度に済ませる。** 空のプロジェクトを作って
    /// 別の操作でリストを入れさせると、その間の状態が意味を持たない。
    #[tracing::instrument(skip(self, display_name), err)]
    pub fn create_project(&mut self, display_name: &str) -> Result<Uuid> {
        let list = generate_single(UnitSet::Core, DEFAULT_UNITS_PER_ROW)?;
        let dir = self.library.create(&Manifest {
            display_name: display_name.to_owned(),
            method: Method::Single,
            item_count: u32::try_from(list.len()).unwrap_or(0),
            derived_from: None,
        })?;
        let mut ledger = Ledger::open(dir.db_path())?;
        ledger.install_reclist(&list, DEFAULT_TONE_MIDI)?;

        // **初回のとっかかりに要る最小限だけ入れる**（TR-RCL-12）。
        // 曲バンクではない。本人が外せる。
        let at = now_rfc3339();
        for (i, song) in ust::bundled_songs().iter().enumerate() {
            ledger.put_song(&format!("bundled-{i}"), song, true, &at)?;
        }
        Ok(dir.id())
    }

    /// プロジェクトを開く。**収録セッションを1つ始める**（`TR-REC-30`）。
    #[tracing::instrument(skip(self), err)]
    pub fn open_project(&mut self, id: Uuid) -> Result<()> {
        let dir = self.library.open_project(id)?;
        let ledger = Ledger::open(dir.db_path())?;
        self.open = Some(Open {
            dir,
            ledger,
            session_id: 0,
        });
        Ok(())
    }

    /// いまの進み具合。
    #[tracing::instrument(skip(self), err)]
    pub fn progress(&mut self) -> Result<Progress> {
        let name = self.display_name()?;
        let open = self.opened_mut()?;
        let covered = open.ledger.covered_units()?;
        let next_row = open.ledger.next_row()?;
        let handoff = if open.ledger.has_been_exported()? {
            HandoffState::Exported
        } else {
            HandoffState::NotExported
        };

        let required: std::collections::BTreeSet<String> =
            koeru_core::inventory::units(UnitSet::Core)
                .iter()
                .map(|u| u.kana.to_owned())
                .collect();

        // **oto の検証はまだ通していない。** 全部録れても AwaitingOto で止まる。
        let coverage = koeru_core::project::coverage_state(&required, &covered, false, &name);

        // **いま歌える曲の数**（TR-RCL-19）。曲が1本も無くても進捗は読める。
        let status = self.song_status()?;

        Ok(Progress {
            next_row,
            covered: covered.len(),
            required: required.len(),
            coverage,
            handoff,
            singable_songs: song::singable_count(&status),
            songs_in_bank: status.len(),
        })
    }

    /// 曲ごとの状態（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
    ///
    /// **収録済み単位が増えるたびに再計算する**（`TR-RCL-17`）。
    /// 手が届く順に並ぶ（追加項目が少ない順、同数なら短い順）。
    #[tracing::instrument(skip(self), err)]
    pub fn song_status(&mut self) -> Result<Vec<SongStatus>> {
        let open = self.opened_mut()?;
        let covered = open.ledger.covered_units()?;
        let songs: Vec<Song> = open
            .ledger
            .songs_in_bank()?
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        Ok(song::status_of(
            &songs,
            koeru_core::alias::Method::Single,
            &covered,
            UnitSet::Core,
        ))
    }

    /// UST を取り込む（`TR-RCL-12`）。
    ///
    /// **主経路はこれ。** 曲バンクを持たないので、何を目標にするかは本人が決める。
    /// **取り込んだ曲データは配布パッケージに含めない。**
    #[tracing::instrument(skip(self, bytes), fields(len = bytes.len(), title), err)]
    pub fn import_ust(&mut self, bytes: &[u8], title: &str) -> Result<String> {
        let song = ust::parse_ust(bytes, title).map_err(|e| AppError::new(e.kind(), e))?;
        let id = Uuid::new_v4().to_string();
        let at = now_rfc3339();
        self.opened_mut()?.ledger.put_song(&id, &song, false, &at)?;
        Ok(id)
    }

    /// 曲をバンクから外す／戻す（`TR-RCL-12`）。**曲そのものは消さない。**
    #[tracing::instrument(skip(self), err)]
    pub fn set_song_in_bank(&mut self, id: &str, in_bank: bool) -> Result<()> {
        self.opened_mut()?.ledger.set_song_in_bank(id, in_bank)?;
        Ok(())
    }

    /// 入力デバイスを挙げる。
    #[tracing::instrument(err)]
    pub fn devices() -> Result<Vec<koeru_audio::DeviceInfo>> {
        Ok(mac::enumerate_input_devices()?)
    }

    /// デバイスを選び、ストリームを開く（`recording-input.fsl` の手順）。
    ///
    /// **ストリームはテイクごとに開閉しない**（`REQ-REC-102`）。
    /// 収録画面を離れるまで持ち続ける。
    #[tracing::instrument(skip(self), err)]
    pub fn arm_device(&mut self, device: &DeviceId) -> Result<mac::MicrophoneMode> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                "収録中はマイクを変えられない",
            ));
        }

        // **前のストリームを先に落とす。** 2つの AUHAL を同時に回さない。
        // 排出スレッドが先。Consumer を握ったまま Capture を捨てない。
        self.pump = None;
        self.capture = None;

        // **状態機械を作り直す。** `recording-input.fsl` の `select_device` は
        // 未選択からしか進めない（`proved`）。マイクの選び直しは、その機械から見れば
        // 「収録画面を出て入り直す」ことなので、機械ごと新しくするのが忠実な読み。
        // **既存の機械を無理に巻き戻さない。** 巻き戻す遷移は仕様に無い。
        self.session = Session::new(3);

        let open = self.opened_mut()?;
        // **セッションは録音条件のスナップショット**（TR-REC-30）。
        let (cap, consumer) = mac::open(device, 48_000 * RING_SECONDS)?;
        let format = cap.format();
        let mode = mac::active_microphone_mode();

        let session_id = open.ledger.start_session(&SessionSnapshot {
            started_at: now_rfc3339(),
            device_id: device.as_str().to_owned(),
            sample_rate_hz: i32::try_from(format.sample_rate_hz).unwrap_or(0),
            channels: i32::from(format.channels),
            effects_state: if mode.is_clean() {
                "clean"
            } else {
                "processed"
            }
            .to_owned(),
            route: "coreaudio-halinput".to_owned(),
        })?;
        open.session_id = session_id;

        // 状態機械を手順どおりに進める。
        self.session.select_device(device.clone())?;
        self.session.open_stream()?;
        if mode.is_clean() {
            self.session.effects_all_disabled()?;
        } else {
            self.session.effects_some_remain()?;
            // **提示は一度だけ**（TR-REC-12）。何度も出すと録音の邪魔になる。
            self.session.show_prompt_once()?;
        }
        self.session.calibrate_gain()?;

        // **アプリが触る前のゲインを覚えておく**（TR-REC-15）。
        // 終了時にここへ戻す。戻せないと、利用者のマイクの設定を勝手に変えたままになる。
        if self.gain_before.as_ref().is_none_or(|(d, _)| d != device)
            && let Some(g) = mac::read_gain(device)
        {
            self.gain_before = Some((device.clone(), g));
        }
        self.device = Some(device.clone());

        // **収録画面に入った時点から止めない**（REQ-REC-102、TR-REC-19）。
        // ここから排出が回り、プリロールが溜まりはじめる。
        cap.arm();
        self.pump = Some(Pump::start(consumer, format.sample_rate_hz));
        self.capture = Some(cap);
        self.estimate_space()?;
        Ok(mode)
    }

    /// 残り全部を録り切れるかを見積もる（`REQ-REC-110`, `TR-REC-41`）。
    ///
    /// **入る分だけ録らせる。** 3時間の収録の途中で埋まると、その日の作業を失う。
    /// 判定は状態機械が一度だけ行い、選ばせない。
    ///
    /// **残量を引けない環境では「足りる」として通す。** 引けないだけで
    /// 収録できなくなるほうが困る（`TR-REC-24` は残量不足を止めるもので、
    /// 残量が読めないことを止めるものではない）。
    #[tracing::instrument(skip(self), err)]
    pub fn estimate_space(&mut self) -> Result<SpaceEstimate> {
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;
        let root = self.opened()?.dir.root().to_path_buf();
        let remaining = self.opened_mut()?.ledger.remaining_rows()?;

        let required = storage::required_bytes(remaining, rate);
        let available = storage::available_bytes(&root);
        self.session
            .estimate_space(required, available.unwrap_or(u64::MAX))?;

        // **足りないときは「その残量で何件録れるか」を出す**（TR-REC-41）。
        // 「足りません」だけでは、何を削れば足りるのか分からない。
        let fits = available.map_or(remaining, |a| storage::rows_that_fit(a, rate));
        Ok(SpaceEstimate {
            remaining_rows: remaining,
            required_bytes: required,
            available_bytes: available,
            rows_that_fit: fits.min(remaining),
        })
    }

    /// 次のテイクを始めてよいだけの残量があるか（`TR-REC-41`）。
    ///
    /// **進行中のテイクは最後まで録りきる。** 止めるのは次を始めるところだけ。
    #[tracing::instrument(skip(self), err)]
    pub fn has_room_for_one_more(&mut self) -> Result<bool> {
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;
        let root = self.opened()?.dir.root().to_path_buf();
        // 引けない環境では止めない。
        let Some(available) = storage::available_bytes(&root) else {
            return Ok(true);
        };
        Ok(available >= storage::required_bytes(1, rate))
    }

    /// 入力が届いているかを確かめる（`TR-REC-17`）。    /// 入力が届いているかを確かめる（`TR-REC-17`）。
    ///
    /// **権限が無いと macOS は無音を返す。** 成否ではなく中身を見る。
    ///
    /// ストリームは開いたまま測る。**止めて測ると、そのぶんプリロールが途切れる**
    /// （`TR-REC-19`）。
    #[tracing::instrument(skip(self), err)]
    pub fn probe_input(&mut self, ms: u64) -> Result<f32> {
        {
            // 直前の残りを捨ててから測る。**「今」の入力だけを見る。**
            let pump = self.pump.as_ref().ok_or_else(no_stream)?;
            let _ = pump.take_peak();
        }
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let peak = self.pump.as_ref().ok_or_else(no_stream)?.take_peak();

        if peak > 1e-6 {
            self.session.input_is_alive()?;
        } else {
            self.session.input_is_dead()?;
        }
        Ok(peak)
    }

    /// プリロールがどれだけ溜まっているか（ミリ秒、`TR-REC-19`）。
    ///
    /// **`PREROLL_MS` に足りていなければ、遡れるのはその長さまで。**
    #[must_use]
    pub fn preroll_ms(&self) -> u64 {
        self.pump.as_ref().map_or(0, Pump::preroll_ms)
    }

    /// 出力がどこへ出ているらしいか（`TR-REC-24`）。
    ///
    /// **これは一次の足切りでしかない。** `TransportType` も `DataSource` も
    /// ドライバの自己申告で、Unknown が正規値として存在する。
    /// 実際の回り込みは [`Studio::check_guide_leak`] が録った音で確かめる。
    #[must_use]
    pub fn output_kind() -> mac::OutputKind {
        mac::default_output_kind()
    }

    /// ガイドを鳴らしながら録って、回り込みを確かめる（`TR-REC-24`）。
    ///
    /// **出力経路の判定だけでは足りない。** ヘッドホンと申告していても、
    /// 装着されている保証はない。**回り込みは録音側でしか確認できない。**
    ///
    /// **これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。**
    ///
    /// 既知の再生信号との相関を取るだけなので、声質の評価を一切含まない
    /// （`TR-REC-17` と同じ性質の静的な経路検査）。
    #[tracing::instrument(skip(self), err)]
    pub fn check_guide_leak(&mut self, midi: i32) -> Result<LeakCheck> {
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;

        // スピーカと分かっているなら、鳴らすまでもなく漏れる。
        if Self::output_kind().definitely_speakers() {
            let found = LeakCheck {
                correlation: 1.0,
                lag_ms: 0.0,
                leaking: true,
            };
            self.leak = Some(found);
            self.session.check_guide_leak()?;
            return Ok(found);
        }

        // 1秒ぶんのガイドを鳴らしながら録る。
        let spec = GuideSpec {
            moras: 2,
            lead_in_ms: 0.0,
            tail_ms: 0.0,
            ..GuideSpec::default()
        };
        let played = guide::render(&spec, midi, rate);
        let captured = self.play_and_capture(&played, rate)?;

        let found = leak::detect(&played, &captured, rate);
        tracing::info!(
            correlation = found.correlation,
            lag_ms = found.lag_ms,
            leaking = found.leaking,
            "回り込みを確かめた"
        );
        self.leak = Some(found);
        self.session.check_guide_leak()?;
        Ok(found)
    }

    /// 音高を鳴らす（`TR-REC-23` の音高提示）。
    ///
    /// **回り込みが確かめられていなければ鳴らさない**（`TR-REC-24`）。
    /// 鳴らしたものが全テイクに混じる。
    #[tracing::instrument(skip(self), err)]
    pub fn play_pitch(&mut self, midi: i32) -> Result<()> {
        match self.leak {
            None => {
                return Err(AppError::new(
                    "recording.leak_unchecked",
                    "先に回り込みを確かめてほしい",
                ));
            }
            Some(l) if l.leaking => {
                return Err(AppError::new(
                    "recording.guide_leaks",
                    "ガイドが録音へ回り込むので鳴らさない",
                ));
            }
            Some(_) => {}
        }
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;
        let pcm = guide::render(&GuideSpec::pitch_reference(), midi, rate);
        self.playback = None;
        self.playback = Some(mac::play(pcm, rate)?);
        Ok(())
    }

    /// 鳴らしながら録る。**回り込みの検査にだけ使う。**
    fn play_and_capture(&mut self, played: &[f32], rate: u32) -> Result<Vec<f32>> {
        let pump = self.pump.as_ref().ok_or_else(no_stream)?;
        pump.begin_probe();
        let handle = mac::play(played.to_vec(), rate)?;

        // 鳴っているあいだ待つ。**余裕を持たせる**（バッファのぶん遅れる）。
        let ms = (played.len() as u64 * 1000 / u64::from(rate.max(1))) + 200;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        drop(handle);

        Ok(self.pump.as_ref().ok_or_else(no_stream)?.end_probe())
    }

    /// 保存してある校正と、いまのゲインを突き合わせる（`TR-REC-15`）。
    ///
    /// **勝手に戻さない。** 差があることを返すだけで、戻すかどうかは本人が決める。
    #[tracing::instrument(skip(self), err)]
    pub fn gain_drift(&mut self) -> Result<Option<(f32, f32)>> {
        let Some(device) = self.device.clone() else {
            return Ok(None);
        };
        let saved = self
            .opened_mut()?
            .ledger
            .calibration_of(device.as_str())?
            .and_then(|c| c.gain);
        let (Some(saved), Some(now)) = (saved, mac::read_gain(&device)) else {
            return Ok(None);
        };
        // 1% 未満の差は動いていないものとして扱う。**OS 側の丸めで毎回出す意味は無い。**
        if (saved - now).abs() < 0.01 {
            Ok(None)
        } else {
            Ok(Some((saved, now)))
        }
    }

    /// 保存してあるゲインへ戻す（`TR-REC-15`）。**本人が選んだときだけ呼ぶ。**
    #[tracing::instrument(skip(self), err)]
    pub fn restore_saved_gain(&mut self) -> Result<()> {
        let Some(device) = self.device.clone() else {
            return Err(no_stream_err());
        };
        let saved = self
            .opened_mut()?
            .ledger
            .calibration_of(device.as_str())?
            .and_then(|c| c.gain);
        if let Some(g) = saved {
            mac::write_gain(&device, g)?;
        }
        Ok(())
    }

    /// 入力レベルを校正する（`TR-REC-14`）。
    ///
    /// **そのプロジェクトで最も高い音高の全力発声**を数秒録って、
    /// ピークが -12〜-6 dBFS に入っていれば校正完了。範囲外なら OS の入力ゲインを動かす。
    ///
    /// **関門にしない。** 収束しなくても収録に進める。3時間の収録の前に、
    /// レベル合わせで止められる方がよほど困る。
    ///
    /// **収録中は呼ばない**（`TR-REC-15`）。
    #[tracing::instrument(skip(self), err)]
    pub fn calibrate(&mut self, seconds: f64) -> Result<Calibration> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                "収録中はゲインを変えない",
            ));
        }
        let device = self.device.clone().ok_or_else(no_stream_err)?;
        let control = mac::gain_control(&device);

        let mut attempt = 1;
        let (peak_dbfs, settled) = loop {
            let peak = self.measure_peak(seconds)?;
            let db = if peak > 0.0 {
                20.0 * f64::from(peak).log10()
            } else {
                f64::NEG_INFINITY
            };

            // **ソフトウェアのボリュームは校正に使えない**（TR-REC-14）。
            // 値は読めても動かさない。動かしても A/D の手前は変わらない。
            let gain = control
                .is_usable()
                .then(|| mac::read_gain(&device))
                .flatten();

            match calibration::step(db, gain, attempt) {
                Outcome::Settled => break (db, true),
                Outcome::Adjust { next_gain } => {
                    tracing::info!(attempt, next_gain, "ゲインを動かして測り直す");
                    mac::write_gain(&device, next_gain)?;
                    attempt += 1;
                }
                Outcome::GaveUp { reason } => {
                    // **ここでも収録には進める。** 関門にしない（TR-REC-14）。
                    // `NoControl` のときは自動調整せず、OS 設定での案内を
                    // 画面側が1回だけ出す（結果の `control` から判断できる）。
                    tracing::info!(reason = reason.as_str(), "校正を切り上げる");
                    break (db, false);
                }
            }
        };

        let result = Calibration {
            gain: control
                .is_usable()
                .then(|| mac::read_gain(&device))
                .flatten(),
            control: control.as_str().to_owned(),
            peak_dbfs,
            settled,
            device_id: device.as_str().to_owned(),
        };
        let at = now_rfc3339();
        self.opened_mut()?.ledger.put_calibration(&result, &at)?;
        // **校正の直後は状態機械の上でも校正済みにする。**
        Ok(result)
    }

    /// 指定した秒数のあいだのピークを測る。
    fn measure_peak(&mut self, seconds: f64) -> Result<f32> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "秒数は 3.0..=5.0 の想定。clamp してから丸める"
        )]
        let ms = (seconds.clamp(0.5, 30.0) * 1000.0) as u64;
        let pump = self.pump.as_ref().ok_or_else(no_stream_err)?;
        let _ = pump.take_peak();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(self.pump.as_ref().ok_or_else(no_stream_err)?.take_peak())
    }

    /// いま録るべき行の収録を始める。
    ///
    /// **押した瞬間より前へ遡って書きはじめる**（`TR-REC-19`）。
    /// 人は「録音」を押してから息を吸わない。指示の時点から書くと語頭が欠ける。
    #[tracing::instrument(skip(self), err)]
    pub fn start_take(&mut self) -> Result<String> {
        if self.recording.is_some() {
            return Err(AppError::new("app.already_recording", "すでに収録中"));
        }
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;
        let audio_dir = self.opened()?.dir.audio_dir();
        let row_id = self
            .opened_mut()?
            .ledger
            .next_row()?
            .ok_or_else(|| AppError::new("app.nothing_to_record", "録るべき行がもう無い"))?
            .0;

        // **世代を名前に入れる。** 録り直しても既存の WAV を上書きしない（TR-PKG-39）。
        let generation = self.opened_mut()?.ledger.takes_of(&row_id)?.len() + 1;
        let path = audio_dir.join(format!("{row_id}_{generation}.wav"));

        // **残りが1テイクぶんを割ったら、次を始めさせない**（TR-REC-41）。
        // 進行中のテイクは最後まで録りきるので、止めるのはここだけ。
        if !self.has_room_for_one_more()? {
            return Err(AppError::new(
                "recording.not_enough_space",
                "保存先の残量が1テイクぶんを割った",
            ));
        }

        // **遡れる分が足りないことは止める理由にしない。** 記録して進む。
        // 収録画面に入った直後は、まだプリロールが溜まりきっていない。
        let held = self.preroll_ms();
        if held < PREROLL_MS {
            tracing::warn!(
                held_ms = held,
                want_ms = PREROLL_MS,
                "プリロールが溜まりきっていない"
            );
        }

        self.session.start_take()?;
        // **ここで取りこぼしの基準を取る。** このテイクの中で増えたぶんだけを見る
        //（TR-REC-07 は「1テイクの中で1フレームでも欠落したら」と定めている）。
        self.xrun_baseline = self
            .capture
            .as_ref()
            .map_or(0, mac::Capture::discontinuities);

        self.pump
            .as_ref()
            .ok_or_else(no_stream)?
            .start_take(path, rate)
            .map_err(|e| AppError::new(e.kind(), e))?;

        self.recording = Some(row_id.clone());
        Ok(row_id)
    }

    /// 収録を止めて、テイクを確定させる。
    ///
    /// **順序は、ファイル確定 → DB コミット**（`DEC-REC-004`）。
    /// 逆にすると、ファイルの無い行が DB に残る。
    ///
    /// 確定のあと、その場で解析と `.frq` と oto の導出まで済ませる
    /// （`TR-PKG-05`, `TR-PKG-42`）。
    ///
    /// **取りこぼしがあったテイクは、ここで自動的に無効にする**（`TR-REC-07`）。
    /// 同じフレーズがもう一度出てくる。
    #[tracing::instrument(skip(self), err)]
    pub fn finish_take(&mut self) -> Result<TakeResult> {
        let row_id = self
            .recording
            .take()
            .ok_or_else(|| AppError::new("app.not_recording", "収録していない"))?;

        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;

        // **指示のあとも `TAIL_MS` ぶん書く**（TR-REC-19）。ここで待つ。
        let finished = self
            .pump
            .as_ref()
            .ok_or_else(no_stream)?
            .finish_take()
            .map_err(|e| AppError::new(e.kind(), e))?;

        // 取りこぼしは、このテイクの中で増えたぶんだけを見る。
        let discontinuities = self
            .capture
            .as_ref()
            .map_or(0, mac::Capture::discontinuities)
            .saturating_sub(self.xrun_baseline);

        self.session.finish_take()?;

        // ── ここまでで**ファイルは確定している**。DB はこの先 ──
        let root = self.opened()?.dir.root().to_path_buf();
        let rel = finished
            .path
            .strip_prefix(&root)
            .unwrap_or(&finished.path)
            .to_string_lossy()
            .into_owned();
        let frames = finished.samples.len();
        let session_id = self.opened()?.session_id;

        let take_id = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: row_id.clone(),
            session_id,
            rel_path: rel,
            frames: i64::try_from(frames).unwrap_or(i64::MAX),
            recorded_at: now_rfc3339(),
        })?;

        // ── 解析。**録音停止時に確定させて、以後 WAV を読み直さない** ──
        let f64s: Vec<f64> = finished.samples.iter().map(|s| f64::from(*s)).collect();
        // **試唱のために走らせる解析を、そのまま .frq へ回す**（TR-PKG-05）。
        // 書き出しのために推定し直さない。
        const F0_PERIOD_MS: f64 = 5.0;
        let (source_f0, _t) = koeru_synth::world::estimate_f0(
            &f64s,
            rate,
            koeru_synth::world::F0Method::Harvest,
            SOURCE_F0_FLOOR_HZ,
            SOURCE_F0_CEIL_HZ,
            F0_PERIOD_MS,
        );
        let analysis =
            TakeAnalysis::compute(&finished.samples, rate, &source_f0, F0_PERIOD_MS / 1000.0);
        self.opened_mut()?.ledger.put_analysis(take_id, &analysis)?;
        analysis.frq.write(&frq::frq_path(&finished.path)?)?;

        // ── 境界と oto ──
        let duration_ms = frames as f64 * 1000.0 / f64::from(rate);
        let cfg = SegmentConfig::default();
        let boundaries = detect_single(&f64s, rate, &cfg);

        // ── 計測（TR-REC-16）と無音マージン（TR-REC-38）──
        // **測るだけ。判定も指摘もしない。**
        let metrics = TakeMetrics::measure(
            &finished.samples,
            rate,
            boundaries.as_ref().map(|b| b.voice_start_ms),
            boundaries.as_ref().map(|b| b.vowel_end_ms),
        );
        self.opened_mut()?.ledger.put_metrics(
            take_id,
            &metrics,
            discontinuities,
            finished.preroll_frames,
        )?;

        let (oto, conf) = match boundaries {
            None => (None, None),
            Some(b) => {
                let c = confidence(&f64s, rate, &b, &cfg).score();
                let o = derive_cv(
                    b.voice_start_ms,
                    b.vowel_start_ms,
                    b.vowel_end_ms,
                    duration_ms,
                    &OtoPreset::default(),
                    // **単独音の1テイクでは無声破裂音かどうかを行から引く。**
                    self.is_unvoiced_plosive(&row_id)?,
                );
                self.opened_mut()?.ledger.put_oto(
                    take_id,
                    &koeru_oto::Oto {
                        offset_ms: o.offset_ms,
                        consonant_ms: o.consonant_ms,
                        cutoff_ms: o.cutoff_ms,
                        preutterance_ms: o.preutterance_ms,
                        overlap_ms: o.overlap_ms,
                    },
                    c,
                    false,
                )?;
                (Some(o), Some(c))
            }
        };

        // ── 採否 ──
        //
        // **取りこぼしたテイクは自動的に無効にする**（TR-REC-07）。
        // 欠落した素材は oto の導出も合成も救えないので、採用の候補に入れない。
        // ファイルは残す（TR-REC-21 の「削除も上書きもしない」）。
        if discontinuities > 0 {
            tracing::warn!(discontinuities, "取りこぼしたテイクを無効にする");
            self.opened_mut()?.ledger.invalidate_take(take_id)?;
        } else {
            // **録れたものは既定で採用する。** 選ばせるのは録り直したときだけ。
            self.opened_mut()?.ledger.adopt_take(&row_id, take_id)?;
        }

        Ok(TakeResult {
            take_id,
            row_id,
            duration_ms,
            peak: analysis.peak,
            thumbnail: analysis.thumbnail,
            oto,
            confidence: conf,
            discontinuities,
            invalidated: discontinuities > 0,
            metrics,
            preroll_ms: finished.preroll_frames as f64 * 1000.0 / f64::from(rate),
        })
    }

    /// 収録済みのテイクを、指定した音高で合成する。**鳴らさない。**
    ///
    /// **周波数表は台帳から取る。** 書き出しのためだけでなく、試唱もここを使う
    /// （`TR-PKG-05` の「再推定を要しない」）。
    ///
    /// 返るのは `(サンプル, サンプルレート)`。
    #[tracing::instrument(skip(self), err)]
    pub fn render_take(
        &mut self,
        take_id: i32,
        midi: i32,
        length_ms: f64,
    ) -> Result<(Vec<f32>, u32)> {
        let root = self.opened()?.dir.root().to_path_buf();
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("app.unknown_take", "そのテイクが台帳に無い"))?;
        let oto = self
            .opened_mut()?
            .ledger
            .oto_of(take_id)?
            .ok_or_else(|| AppError::new("app.no_oto", "そのテイクにまだ原音設定が無い"))?;
        let analysis = self.opened_mut()?.ledger.analysis_of(take_id)?;

        let w = wav::read(root.join(&take.rel_path))?;
        let samples: Vec<f64> = w.samples.iter().map(|s| f64::from(*s)).collect();
        let table = analysis.map(|a| a.frq.f0).unwrap_or_default();

        let out = render(&RenderRequest {
            samples: &samples,
            sample_rate_hz: w.rate_hz,
            // **`tone` は鳴らしたい音高。収録音高ではない**（resampler の doc を参照）。
            // ここに収録音高を渡すと、どの音高を選んでも同じ高さで鳴る。
            tone: midi,
            oto: Oto {
                offset_ms: oto.offset_ms,
                consonant_ms: oto.consonant_ms,
                cutoff_ms: oto.cutoff_ms,
                preutterance_ms: oto.preutterance_ms,
                overlap_ms: oto.overlap_ms,
            },
            required_length_ms: length_ms,
            consonant_velocity: 100.0,
            volume: 100.0,
            modulation: 0.0,
            tempo: 120.0,
            pitch_bend_cents: &[],
            frequency_table: &table,
        })?;

        #[allow(
            clippy::cast_possible_truncation,
            reason = "合成結果は -1.0..=1.0 付近。f32 で鳴らす"
        )]
        let pcm: Vec<f32> = out.iter().map(|v| *v as f32).collect();
        Ok((pcm, w.rate_hz))
    }

    /// 収録済みのテイクを、指定した音高で鳴らす。**縦切りの終点。**
    #[tracing::instrument(skip(self), err)]
    pub fn preview(&mut self, take_id: i32, midi: i32, length_ms: f64) -> Result<usize> {
        let (pcm, rate) = self.render_take(take_id, midi, length_ms)?;
        let n = pcm.len();

        // **前の再生は止める。** 重ねると何を聴いているか分からなくなる。
        self.playback = None;
        self.playback = Some(mac::play(pcm, rate)?);
        Ok(n)
    }

    /// 鳴らしている音を止める。
    pub fn stop_preview(&mut self) {
        self.playback = None;
    }

    /// **テスト用。** 音声デバイス無しで、行を収録済みとして印を付ける。
    ///
    /// 実際の収録は `start_take` → `finish_take` を通る。ここはカバレッジの
    /// 計算だけを確かめたいときの入口。
    #[cfg(any(test, feature = "test-hooks"))]
    #[tracing::instrument(skip(self), err)]
    pub fn mark_recorded_for_test(&mut self, row_id: &str) -> Result<()> {
        let session_id = {
            let open = self.opened_mut()?;
            if open.session_id == 0 {
                open.session_id = open.ledger.start_session(&SessionSnapshot {
                    started_at: now_rfc3339(),
                    device_id: "test".to_owned(),
                    sample_rate_hz: 44_100,
                    channels: 1,
                    effects_state: "clean".to_owned(),
                    route: "test".to_owned(),
                })?;
            }
            open.session_id
        };
        let take = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: format!("audio/{row_id}_1.wav"),
            frames: 44_100,
            recorded_at: now_rfc3339(),
        })?;
        self.opened_mut()?.ledger.adopt_take(row_id, take)?;
        Ok(())
    }

    /// 開いているプロジェクトのディレクトリ。
    #[tracing::instrument(skip(self), err)]
    pub fn project_dir(&self) -> Result<&ProjectDir> {
        Ok(&self.opened()?.dir)
    }

    fn display_name(&self) -> Result<String> {
        Ok(self.opened()?.dir.read_manifest()?.display_name)
    }

    fn is_unvoiced_plosive(&mut self, row_id: &str) -> Result<bool> {
        let kana = self.opened_mut()?.ledger.units_of(row_id)?;
        let all = koeru_core::inventory::units(UnitSet::Core);
        Ok(kana.iter().any(|k| {
            all.iter()
                .find(|u| u.kana == k)
                .is_some_and(|u| u.unvoiced_plosive)
        }))
    }

    fn opened(&self) -> Result<&Open> {
        self.open
            .as_ref()
            .ok_or_else(|| AppError::new("app.no_project", "プロジェクトを開いていない"))
    }

    fn opened_mut(&mut self) -> Result<&mut Open> {
        self.open
            .as_mut()
            .ok_or_else(|| AppError::new("app.no_project", "プロジェクトを開いていない"))
    }
}

fn no_stream() -> AppError {
    AppError::new("app.no_stream", "入力ストリームを開いていない")
}

/// `ok_or_else` に渡すための同じもの。
fn no_stream_err() -> AppError {
    no_stream()
}

impl Drop for Studio {
    /// **アプリが変更した OS 側のゲインを、終了時に元へ戻す**（`TR-REC-15`）。
    ///
    /// 戻さないと、利用者のマイクの設定を勝手に変えたままになる。
    /// KOERU を閉じたあとに別のアプリで小さすぎる／大きすぎる音になる。
    fn drop(&mut self) {
        // 排出スレッドを先に止める。ゲインを触るのはそのあと。
        self.pump = None;
        if let Some((device, before)) = self.gain_before.take()
            && let Err(e) = mac::write_gain(&device, before)
        {
            tracing::warn!(kind = e.kind(), "終了時にゲインを戻せなかった");
        }
    }
}

/// 現在時刻を RFC 3339 で。
///
/// **秒までで足りる。** 台帳に入るのは順序を保つためで、精密な時刻ではない。
fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 素の算術で組む。**日付だけのために依存を増やさない。**
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 1970-01-01 からの日数を年月日にする（Howard Hinnant の `civil_from_days`）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
