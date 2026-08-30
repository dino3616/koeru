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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use koeru_audio::backend::macos as mac;
use koeru_audio::{DeviceId, Session, ring, wav};
use koeru_core::analysis::TakeAnalysis;
use koeru_core::db::{FinalizedTake, Ledger, SessionSnapshot, koeru_oto};
use koeru_core::frq;
use koeru_core::inventory::UnitSet;
use koeru_core::project::{CoverageState, HandoffState, Library, Manifest, Method, ProjectDir};
use koeru_core::reclist::{DEFAULT_UNITS_PER_ROW, generate_single};
use koeru_synth::oto::{Oto, OtoPreset, derive_cv};
use koeru_synth::resampler::{RenderRequest, render};
use koeru_synth::segment::{SegmentConfig, confidence, detect_single};

use uuid::Uuid;

use crate::error::{AppError, Result};
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

/// 録音中のテイク。
///
/// **`PartialTake` はスレッドが所有する。** 排出はコールバックと別スレッドで回し、
/// 止めるときに join して受け取る。
struct Recording {
    row_id: String,
    stop: Arc<AtomicBool>,
    pump: JoinHandle<std::result::Result<Drained, wav::WavError>>,
}

impl std::fmt::Debug for Recording {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Recording")
            .field("row_id", &self.row_id)
            .finish_non_exhaustive()
    }
}

/// 排出スレッドが返すもの。
struct Drained {
    consumer: ring::Consumer,
    path: PathBuf,
    samples: Vec<f32>,
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
    /// 取りこぼしの回数。**0 でなければ録り直しを促す**（`TR-REC-07`）。
    pub discontinuities: usize,
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
}

/// 縦切りの本体。
#[derive(Debug)]
pub struct Studio {
    library: Library,
    open: Option<Open>,
    capture: Option<mac::Capture>,
    consumer: Option<ring::Consumer>,
    session: Session,
    recording: Option<Recording>,
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
            consumer: None,
            // 3件までの提示上限は `recording-input.fsl` と揃える。
            session: Session::new(3),
            recording: None,
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

        Ok(Progress {
            next_row,
            covered: covered.len(),
            required: required.len(),
            coverage,
            handoff,
        })
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
        self.capture = None;
        self.consumer = None;

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

        self.capture = Some(cap);
        self.consumer = Some(consumer);
        self.estimate_space()?;
        Ok(mode)
    }

    /// 残り全部を録り切れるかを見積もる（`REQ-REC-110`）。
    ///
    /// **入る分だけ録らせる。** 3時間の収録の途中で埋まると、その日の作業を失う。
    /// 判定は状態機械が一度だけ行い、選ばせない。
    ///
    /// **残量を引けない環境では「足りる」として通す。** 引けないだけで
    /// 収録できなくなるほうが困る（`TR-REC-24` は残量不足を止めるもので、
    /// 残量が読めないことを止めるものではない）。
    #[tracing::instrument(skip(self), err)]
    pub fn estimate_space(&mut self) -> Result<u64> {
        let rate = self
            .capture
            .as_ref()
            .ok_or_else(no_stream)?
            .format()
            .sample_rate_hz;
        let root = self.opened()?.dir.root().to_path_buf();
        let remaining = self.opened_mut()?.ledger.remaining_rows()?;

        let required = storage::required_bytes(remaining, rate);
        let available = storage::available_bytes(&root).unwrap_or(u64::MAX);
        self.session.estimate_space(required, available)?;
        Ok(required)
    }

    /// 入力が届いているかを確かめる（`TR-REC-17`）。
    ///
    /// **権限が無いと macOS は無音を返す。** 成否ではなく中身を見る。
    #[tracing::instrument(skip(self), err)]
    pub fn probe_input(&mut self, ms: u64) -> Result<f32> {
        let cap = self.capture.as_ref().ok_or_else(no_stream)?;
        let consumer = self.consumer.as_ref().ok_or_else(no_stream)?;

        cap.arm();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        cap.disarm();

        let mut buf = vec![0.0_f32; 16_384];
        let mut peak = 0.0_f32;
        loop {
            let n = consumer.pop(&mut buf);
            if n == 0 {
                break;
            }
            for v in &buf[..n] {
                peak = peak.max(v.abs());
            }
        }

        if peak > 1e-6 {
            self.session.input_is_alive()?;
        } else {
            self.session.input_is_dead()?;
        }
        Ok(peak)
    }

    /// いま録るべき行の収録を始める。
    ///
    /// **`.wav.part` を開いて、排出スレッドを回す。** リングはコールバックが
    /// 埋めるので、ディスクへ落とす側を別に持たないと溢れる。
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

        let part = wav::PartialTake::create(&path, rate)?;
        let consumer = self.consumer.take().ok_or_else(no_stream)?;
        let stop = Arc::new(AtomicBool::new(false));

        self.session.start_take()?;
        self.capture.as_ref().ok_or_else(no_stream)?.arm();

        let pump = std::thread::spawn({
            let stop = Arc::clone(&stop);
            move || pump(consumer, part, path, &stop)
        });

        self.recording = Some(Recording {
            row_id: row_id.clone(),
            stop,
            pump,
        });
        Ok(row_id)
    }

    /// 収録を止めて、テイクを確定させる。
    ///
    /// **順序は、ファイル確定 → DB コミット**（`DEC-REC-004`）。
    /// 逆にすると、ファイルの無い行が DB に残る。
    ///
    /// 確定のあと、その場で解析と `.frq` と oto の導出まで済ませる
    /// （`TR-PKG-05`, `TR-PKG-42`）。
    #[tracing::instrument(skip(self), err)]
    pub fn finish_take(&mut self) -> Result<TakeResult> {
        let rec = self
            .recording
            .take()
            .ok_or_else(|| AppError::new("app.not_recording", "収録していない"))?;

        let cap = self.capture.as_ref().ok_or_else(no_stream)?;
        cap.disarm();
        let discontinuities = cap.discontinuities();
        let rate = cap.format().sample_rate_hz;

        rec.stop.store(true, Ordering::Release);
        let drained = rec
            .pump
            .join()
            .map_err(|_| AppError::new("app.pump_panicked", "排出スレッドが落ちた"))??;
        self.consumer = Some(drained.consumer);
        self.session.finish_take()?;

        // ── ここまでで**ファイルは確定している**。DB はこの先 ──
        let root = self.opened()?.dir.root().to_path_buf();
        let rel = drained
            .path
            .strip_prefix(&root)
            .unwrap_or(&drained.path)
            .to_string_lossy()
            .into_owned();
        let frames = u64::try_from(drained.samples.len()).unwrap_or(0);
        let session_id = self.opened()?.session_id;

        let take_id = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: rec.row_id.clone(),
            session_id,
            rel_path: rel,
            frames: i64::try_from(frames).unwrap_or(i64::MAX),
            recorded_at: now_rfc3339(),
        })?;
        // **録れたものは既定で採用する。** 選ばせるのは録り直したときだけ。
        self.opened_mut()?.ledger.adopt_take(&rec.row_id, take_id)?;

        // ── 解析。**録音停止時に確定させて、以後 WAV を読み直さない** ──
        let f64s: Vec<f64> = drained.samples.iter().map(|s| f64::from(*s)).collect();
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
            TakeAnalysis::compute(&drained.samples, rate, &source_f0, F0_PERIOD_MS / 1000.0);
        self.opened_mut()?.ledger.put_analysis(take_id, &analysis)?;
        analysis.frq.write(&frq::frq_path(&drained.path)?)?;

        // ── 境界と oto ──
        let duration_ms = frames as f64 * 1000.0 / f64::from(rate);
        let cfg = SegmentConfig::default();
        let (oto, conf) = match detect_single(&f64s, rate, &cfg) {
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
                    self.is_unvoiced_plosive(&rec.row_id)?,
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

        Ok(TakeResult {
            take_id,
            row_id: rec.row_id,
            duration_ms,
            peak: analysis.peak,
            thumbnail: analysis.thumbnail,
            oto,
            confidence: conf,
            discontinuities,
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

/// リングから排出して `.wav.part` へ落とし続ける。
///
/// **止まれと言われてから、残りを全部吸い出す。** 途中で切ると末尾が欠ける。
fn pump(
    consumer: ring::Consumer,
    mut part: wav::PartialTake,
    path: PathBuf,
    stop: &AtomicBool,
) -> std::result::Result<Drained, wav::WavError> {
    let mut buf = vec![0.0_f32; 8192];
    let mut all = Vec::new();

    loop {
        let n = consumer.pop(&mut buf);
        if n > 0 {
            part.write(&buf[..n])?;
            all.extend_from_slice(&buf[..n]);
        } else if stop.load(Ordering::Acquire) {
            // **もう一度だけ空にしてから抜ける。**
            let n = consumer.pop(&mut buf);
            if n == 0 {
                break;
            }
            part.write(&buf[..n])?;
            all.extend_from_slice(&buf[..n]);
        } else {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }

    let path = part.finalize().unwrap_or(path);
    Ok(Drained {
        consumer,
        path,
        samples: all,
    })
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
