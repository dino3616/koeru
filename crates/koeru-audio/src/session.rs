//! 収録セッションの状態機械。
//!
//! **`specs/requirements/recording-input.fsl` の写し。** あちらは `proved`（無限深度）で、
//! ここはその契約を Rust で持つ。**振る舞いを足すときは FSL を先に直す。**
//! 対応は `@requirement` の ID で辿れる。
//!
//! この型は純粋で、I/O を持たない。OS 側の出来事はバックエンドが呼び分けて渡す。
//! そうしておくと、**ハードウェアなしで契約を検査できる。**
//!
//! **加工を無効化できない場合の扱いは、指摘ではなく一度きりの提示に限る**
//! （`TR-REC-10`）。何度も出すと、録音そのものの邪魔になる。
//!
//! # 写してはいけないものがある
//!
//! FSL の `MAX_TAKES` は**写さない。** あれは `ASSUME-3`——
//! 「テイク数は検証用に有限へ閉じる（表現上の仮定）」であって、製品の規則ではない。
//! **設計層の `project-storage.fsl` では同じ定数が 2 になっている。**
//! 値が食い違うこと自体が、任意の有界化である証拠。
//!
//! **製品側の規則は逆。** `TR-REC-21` が「録音リスト項目1つあたりのテイク保持数は
//! **上限を設けず**、プロジェクトの総容量が閾値（既定 4 GB）を超えたときに、
//! 非採用テイクの古い順から削除候補として本人に提示する」と定めている。
//!
//! **一度写して踏んだ。** `Session::new(3)` としてアプリに入り、
//! 3テイク録ると以降どの項目も録れなくなっていた（`DEC-REC-005`）。
//! ここで効く上限は**残量だけ**（`space_sufficient` / `TR-REC-41`）。

use crate::device::DeviceId;
use crate::error::SessionError;

type Result<T> = std::result::Result<T, SessionError>;

/// 選択したデバイスの状態（FSL の `Device`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    NotSelected,
    Selected,
    Lost,
}

/// OS 側の音声加工の列挙結果（FSL の `Effects`）。
///
/// ASSUME-2: 個々の効果種別（AGC / NS / AEC 等）はここでは畳む。
/// 種別ごとの記録はセッションメタデータが持つ（TR-REC-08）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effects {
    Unknown,
    Clean,
    SomeRemain,
}

/// 入力レベルの校正状態（FSL の `Gain`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    NotCalibrated,
    Calibrated,
}

/// 入力経路の生死（FSL の `Liveness`）。
///
/// ASSUME-1: 「最初の 1.0 秒間」「-90 dBFS」（TR-REC-17）は実時間と連続量なので、
/// ここでは「届いているか」の離散判定に畳む。閾値の判定はバックエンドが持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Liveness {
    Unchecked,
    Alive,
    Dead,
}

/// 収録セッション。
#[derive(Debug)]
pub struct Session {
    device: Device,
    device_id: Option<DeviceId>,
    stream_open: bool,
    effects: Effects,
    prompts_shown: u32,
    gain: Gain,
    os_gain_changed: bool,
    os_gain_restored: bool,
    guide_enabled: bool,
    leak_checked: bool,
    liveness: Liveness,
    space_estimated: bool,
    space_sufficient: bool,
    recording: bool,
    takes: u32,
    exited: bool,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// FSL の `init` と同じ初期状態。
    ///
    /// **テイク数の上限は取らない**（`TR-REC-21`）。冒頭の「写してはいけないもの」を参照。
    #[must_use]
    pub fn new() -> Self {
        Self {
            device: Device::NotSelected,
            device_id: None,
            stream_open: false,
            effects: Effects::Unknown,
            prompts_shown: 0,
            gain: Gain::NotCalibrated,
            os_gain_changed: false,
            os_gain_restored: false,
            guide_enabled: false,
            leak_checked: false,
            liveness: Liveness::Unchecked,
            space_estimated: false,
            space_sufficient: false,
            recording: false,
            takes: 0,
            exited: false,
        }
    }

    // ── 観測 ────────────────────────────────────────────────
    #[must_use]
    pub const fn device(&self) -> Device {
        self.device
    }
    #[must_use]
    pub fn device_id(&self) -> Option<&DeviceId> {
        self.device_id.as_ref()
    }
    #[must_use]
    pub const fn effects(&self) -> Effects {
        self.effects
    }
    #[must_use]
    pub const fn gain(&self) -> Gain {
        self.gain
    }
    #[must_use]
    pub const fn liveness(&self) -> Liveness {
        self.liveness
    }
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recording
    }
    #[must_use]
    pub const fn is_stream_open(&self) -> bool {
        self.stream_open
    }
    #[must_use]
    pub const fn takes(&self) -> u32 {
        self.takes
    }
    #[must_use]
    pub const fn prompts_shown(&self) -> u32 {
        self.prompts_shown
    }
    #[must_use]
    pub const fn guide_enabled(&self) -> bool {
        self.guide_enabled
    }
    #[must_use]
    pub const fn os_gain_restored(&self) -> bool {
        self.os_gain_restored
    }

    // ── 遷移 ────────────────────────────────────────────────

    /// REQ-REC-101 入力デバイスは本人が明示的に選び、識別子でプロジェクトに固定する。
    pub fn select_device(&mut self, id: DeviceId) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::NotSelected)?;
        tracing::debug!(device = ?id, "デバイスを選択");
        self.device = Device::Selected;
        self.device_id = Some(id);
        self.settled()
    }

    /// REQ-REC-102 収録画面に入った時点でストリームを開き、テイクごとに開閉しない。
    pub fn open_stream(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::Selected)?;
        self.expect_stream(false)?;
        self.stream_open = true;
        self.settled()
    }

    /// REQ-REC-103 適用中の効果を列挙し、無効化できたものは無効化した。
    pub fn effects_all_disabled(&mut self) -> Result<()> {
        self.enumerate_effects(Effects::Clean)
    }

    /// REQ-REC-103 無効化できない効果が残った。
    pub fn effects_some_remain(&mut self) -> Result<()> {
        self.enumerate_effects(Effects::SomeRemain)
    }

    fn enumerate_effects(&mut self, outcome: Effects) -> Result<()> {
        self.alive()?;
        self.expect_stream(true)?;
        if self.effects != Effects::Unknown {
            return Err(SessionError::EffectsState {
                expected: Effects::Unknown,
                actual: self.effects,
            });
        }
        tracing::debug!(effects = ?outcome, "効果の列挙が終わった");
        self.effects = outcome;
        self.settled()
    }

    /// REQ-REC-104 無効化できない効果が残ったら、収録開始前に一度だけ手順を提示する。
    pub fn show_prompt_once(&mut self) -> Result<()> {
        self.alive()?;
        if self.effects != Effects::SomeRemain {
            return Err(SessionError::EffectsState {
                expected: Effects::SomeRemain,
                actual: self.effects,
            });
        }
        if self.prompts_shown > 0 {
            return Err(SessionError::PromptAlreadyShown);
        }
        self.expect_not_recording()?;
        self.prompts_shown += 1;
        self.settled()
    }

    /// REQ-REC-105 入力レベルの校正は収録前の1回のセットアップ工程で、収録中は行わない。
    pub fn calibrate_gain(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::Selected)?;
        self.expect_not_recording()?;
        if self.gain != Gain::NotCalibrated {
            return Err(SessionError::GainState {
                expected: Gain::NotCalibrated,
                actual: self.gain,
            });
        }
        self.gain = Gain::Calibrated;
        // OS 側のゲインを変えた事実を残す。終了時に戻す義務が生じる（INV-REC-107）。
        self.os_gain_changed = true;
        self.settled()
    }

    /// REQ-REC-106 入力が届いていた。
    pub fn input_is_alive(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_stream(true)?;
        self.expect_liveness(Liveness::Unchecked)?;
        self.liveness = Liveness::Alive;
        self.settled()
    }

    /// REQ-REC-106 入力が届いていなければ収録を止め、テイクを作らずデバイス選択へ戻す。
    pub fn input_is_dead(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_stream(true)?;
        self.expect_liveness(Liveness::Unchecked)?;
        tracing::warn!("入力が届いていない。デバイス選択へ戻す");
        self.liveness = Liveness::Dead;
        self.recording = false;
        self.stream_open = false;
        self.device = Device::NotSelected;
        self.device_id = None;
        self.settled()
    }

    /// REQ-REC-107 ガイドを鳴らす前に、回り込みの有無を一度だけ確認する。
    pub fn check_guide_leak(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_stream(true)?;
        self.expect_not_recording()?;
        if self.leak_checked {
            return Ok(());
        }
        self.leak_checked = true;
        self.settled()
    }

    /// REQ-REC-107 回り込みが無いと確認できたときだけガイドを鳴らす。
    pub fn enable_guide(&mut self) -> Result<()> {
        self.alive()?;
        if !self.leak_checked {
            return Err(SessionError::LeakNotChecked);
        }
        if self.guide_enabled {
            return Err(SessionError::GuideAlreadyEnabled);
        }
        self.guide_enabled = true;
        self.settled()
    }

    /// REQ-REC-110 収録を始める前に、リスト全体が必要とする容量を見積もる。
    ///
    /// **足りるかどうかは見積もった結果で決まり、選べない。** 呼び出し側は
    /// 実際の残量と必要量を渡し、判定はここで一度だけ行う。
    pub fn estimate_space(&mut self, required_bytes: u64, available_bytes: u64) -> Result<()> {
        self.alive()?;
        self.expect_not_recording()?;
        let sufficient = available_bytes >= required_bytes;
        if !sufficient {
            tracing::warn!("保存先の残量が足りない。収録を開始させない");
        }
        self.space_estimated = true;
        self.space_sufficient = sufficient;
        self.settled()
    }

    /// REQ-REC-108 / REQ-REC-110 収録は、デバイスが生きていて入力が届き、
    /// 校正済みで、残量が足りているときだけ始められる。
    #[tracing::instrument(skip(self), fields(takes = self.takes), err)]
    pub fn start_take(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::Selected)?;
        self.expect_stream(true)?;
        self.expect_liveness(Liveness::Alive)?;
        if self.gain != Gain::Calibrated {
            return Err(SessionError::GainState {
                expected: Gain::Calibrated,
                actual: self.gain,
            });
        }
        if !(self.space_estimated && self.space_sufficient) {
            return Err(SessionError::NotEnoughSpace);
        }
        self.expect_not_recording()?;
        // **テイク数では止めない**（`TR-REC-21`）。止めるのは残量（上の `NotEnoughSpace`）。
        self.recording = true;
        self.settled()
    }

    /// REQ-REC-108 テイクが確定してもストリームは開いたままにする。
    #[tracing::instrument(skip(self), fields(takes = self.takes), err)]
    pub fn finish_take(&mut self) -> Result<()> {
        self.alive()?;
        if !self.recording {
            return Err(SessionError::RecordingState {
                want_recording: true,
            });
        }
        self.recording = false;
        self.takes += 1;
        self.settled()
    }

    /// REQ-REC-109 選択済みデバイスが録音中に消失したら、進行中テイクを破棄して収録を止める。
    ///
    /// **識別子は保持する。** 同一識別子が戻ったときだけ再開できる。
    pub fn device_lost(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::Selected)?;
        tracing::warn!("収録中にデバイスを失った。進行中テイクを破棄する");
        self.device = Device::Lost;
        self.recording = false;
        self.stream_open = false;
        self.liveness = Liveness::Unchecked;
        self.settled()
    }

    /// REQ-REC-109 復帰は同一識別子のデバイスが戻ったときだけ。
    ///
    /// **別のデバイスへ自動で切り替えない。** 識別子が違えば拒む。
    pub fn same_device_returned(&mut self, id: &DeviceId) -> Result<()> {
        self.alive()?;
        self.expect_device(Device::Lost)?;
        if self.device_id.as_ref() != Some(id) {
            return Err(SessionError::DeviceState {
                expected: Device::Lost,
                actual: Device::Lost,
            });
        }
        self.device = Device::Selected;
        self.settled()
    }

    /// REQ-REC-105 アプリが変更した OS 側のゲインは、終了時に変更前の値へ戻す。
    pub fn exit(&mut self) -> Result<()> {
        self.alive()?;
        self.expect_not_recording()?;
        self.exited = true;
        self.os_gain_restored = self.os_gain_changed;
        self.settled()
    }

    // ── 前提の検査 ──────────────────────────────────────────

    fn alive(&self) -> Result<()> {
        if self.exited {
            return Err(SessionError::Exited);
        }
        Ok(())
    }

    fn expect_device(&self, expected: Device) -> Result<()> {
        if self.device == expected {
            return Ok(());
        }
        Err(SessionError::DeviceState {
            expected,
            actual: self.device,
        })
    }

    fn expect_stream(&self, want_open: bool) -> Result<()> {
        if self.stream_open == want_open {
            return Ok(());
        }
        Err(SessionError::StreamState { want_open })
    }

    fn expect_liveness(&self, expected: Liveness) -> Result<()> {
        if self.liveness == expected {
            return Ok(());
        }
        Err(SessionError::LivenessState {
            expected,
            actual: self.liveness,
        })
    }

    fn expect_not_recording(&self) -> Result<()> {
        if self.recording {
            return Err(SessionError::RecordingState {
                want_recording: false,
            });
        }
        Ok(())
    }

    /// 遷移のたびに不変条件を確かめる。
    ///
    /// **FSL 側で `proved` になっている命題をここでも持つ。** 仕様と実装が
    /// 離れたときに、テストを待たずその場で落ちる。
    fn settled(&self) -> Result<()> {
        debug_assert!(
            self.prompts_shown <= 1,
            "INV-REC-108 手順の提示は多くとも一度"
        );
        debug_assert!(
            self.device == Device::Selected || !self.recording,
            "INV-REC-101 デバイスを失った状態では収録していない"
        );
        debug_assert!(
            !self.recording || self.stream_open,
            "INV-REC-102 収録しているならストリームは開いている"
        );
        debug_assert!(
            !self.recording || self.liveness == Liveness::Alive,
            "INV-REC-103 入力が届いていないまま収録することはない"
        );
        debug_assert!(
            !self.recording || self.gain == Gain::Calibrated,
            "INV-REC-104 校正していないまま収録することはない"
        );
        debug_assert!(
            !self.guide_enabled || self.leak_checked,
            "INV-REC-105 回り込みを確認しないままガイドを鳴らさない"
        );
        debug_assert!(
            !(self.exited && self.os_gain_changed) || self.os_gain_restored,
            "INV-REC-107 変えたゲインは終了時に戻している"
        );
        Ok(())
    }
}
