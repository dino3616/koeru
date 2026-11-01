//! サブバンクの枠（`DEC-RCL-018`）。 一覧・足す・付け替える・並べ替える・担当音域・消す。
//!
//! **どれも収録セッションを始め直さない**（`TR-REC-30`）。 `open_project` を呼ばず、開いている
//! 台帳の書き手だけを使う。 枠を足しても消しても、録っている途中のセッションはそのまま続く。
//!
//! 断る理由は値で返す（`DEC-PLT-038`）。 [`AppError`] になるのは台帳やファイルを扱えないときだけ。

use koeru_core::db::subbank::{
    NewSubbank, PresetChange, SubbankRecord, SubbankRefusal, SubbankRevision,
};
use koeru_core::preset::MethodPreset;
use koeru_model::id::SubbankId;
use koeru_model::subbank::{ToneRange, ToneRangeError, VoiceColor, default_ranges};

use super::{Studio, preset_of};
use crate::error::Result;

/// 画面に出す枠の1つ（SDL の `Subbank`）。
#[derive(Debug, Clone, PartialEq)]
pub struct SubbankSummary {
    pub id: SubbankId,
    /// `None` は声色なし。
    pub voice_color: Option<VoiceColor>,
    pub method: koeru_core::alias::Method,
    /// 作ったときの方式プリセット。 古い project の枠は持たない。
    pub preset_id: Option<String>,
    /// 収録音階（MIDI）。
    pub tone: i32,
    /// 担当音域。 直していなければ floor 割り当ての既定（`TR-RCL-31`）。
    pub range: ToneRange,
    /// 本人が直した範囲か。
    pub range_edited: bool,
    pub rows: usize,
    /// 採用テイクのある行。
    pub recorded_rows: usize,
    /// 固定の見積もり（`DEC-RCL-013`）。
    pub remaining_seconds: f64,
    /// 確定したテイクの数。 無効にしたものも数える——消すと一緒に消えるので。
    pub takes: usize,
}

/// 入力のどこが通らなかったか（SDL の `InputViolation`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Violation {
    /// 入力の中の場所。 SDL の欄の名前。
    pub field: &'static str,
    pub code: &'static str,
}

/// 枠の操作を断った理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubbankDenied {
    /// 台帳が断った。
    Refused(SubbankRefusal),
    /// 入力が通らない。 直さずに送り直しても同じになる。
    Invalid(Violation),
}

impl From<SubbankRefusal> for SubbankDenied {
    fn from(r: SubbankRefusal) -> Self {
        Self::Refused(r)
    }
}

/// 枠の操作の答え。 外側の `Result` は台帳やファイルを扱えなかったこと。
pub type SubbankAnswer<T> = std::result::Result<T, SubbankDenied>;

/// 足す枠（SDL の `AddSubbankInput`）。
#[derive(Debug, Clone, Copy)]
pub struct AddSubbank<'a> {
    /// `None` は声色なし。
    pub voice_color: Option<&'a str>,
    pub preset: &'a str,
    pub tone: i32,
    /// この枠の後ろに置く。 `None` は末尾。
    pub after: Option<SubbankId>,
}

/// 付け替え（SDL の `ReviseSubbankInput`）。 `None` の欄は変えない。
#[derive(Debug, Clone, Copy, Default)]
pub struct ReviseSubbank<'a> {
    pub voice_color: Option<&'a str>,
    /// 声色なしへ戻す。 `voice_color` と同時には指せない。
    pub clear_voice_color: bool,
    pub tone: Option<i32>,
    pub preset: Option<&'a str>,
}

/// 消した結果（SDL の `SubbankRemoved`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubbankRemoval {
    pub subbank: SubbankId,
    pub removed_takes: usize,
}

fn invalid(field: &'static str, code: &'static str) -> SubbankDenied {
    SubbankDenied::Invalid(Violation { field, code })
}

/// 声色の綴り。 空の綴りは声色なしと区別できないので通さない。
fn voice_color(field: &'static str, name: &str) -> SubbankAnswer<VoiceColor> {
    VoiceColor::parse(name).ok_or_else(|| invalid(field, "subbank.empty_voice_color"))
}

fn tone(field: &'static str, midi: i32) -> SubbankAnswer<i32> {
    if koeru_model::subbank::is_recordable_tone(midi) {
        Ok(midi)
    } else {
        Err(invalid(field, "subbank.tone_out_of_range"))
    }
}

fn preset(field: &'static str, id: &str) -> SubbankAnswer<MethodPreset> {
    koeru_core::preset::by_id(id).ok_or_else(|| invalid(field, "preset.unknown"))
}

pub(super) const fn range_violation(e: ToneRangeError) -> Violation {
    let code = match e {
        ToneRangeError::OutOfRange => "subbank.tone_range_out_of_range",
        ToneRangeError::Inverted => "subbank.tone_range_inverted",
    };
    Violation {
        field: "range",
        code,
    }
}

impl Studio {
    /// 枠を並びの順で（SDL の `RecordingFacet.subbanks`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn subbanks(&mut self) -> Result<Vec<SubbankSummary>> {
        let project = preset_of(&self.opened()?.dir().read_manifest()?);
        let mut ledger = self.writer()?;
        let records = ledger.subbanks()?;
        let keys: Vec<(Option<&VoiceColor>, i32)> = records
            .iter()
            .map(|s| (s.voice_color.as_ref(), s.tone))
            .collect();
        let defaults = default_ranges(&keys);
        let mut out = Vec::with_capacity(records.len());
        for (s, default) in records.iter().zip(defaults) {
            let set = s
                .preset_id
                .as_deref()
                .and_then(koeru_core::preset::by_id)
                .unwrap_or(project)
                .set;
            let left = ledger.unrecorded_rows_of(s.id, set)?;
            out.push(summary(s, default, &left));
        }
        Ok(out)
    }

    /// 枠を足す（SDL の `addSubbank`、`TR-RCL-28`）。 足した枠は未収録から始まる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、録音リストを作れない、台帳かディレクトリを書けない。
    #[tracing::instrument(skip(self, input), fields(tone = input.tone))]
    pub fn add_subbank(&mut self, input: &AddSubbank<'_>) -> Result<SubbankAnswer<SubbankSummary>> {
        let color = match input.voice_color.map(|c| voice_color("voiceColor", c)) {
            None => None,
            Some(Ok(c)) => Some(c),
            Some(Err(d)) => return Ok(Err(d)),
        };
        let (preset, tone) = match (preset("preset", input.preset), tone("tone", input.tone)) {
            (Ok(p), Ok(t)) => (p, t),
            (Err(d), _) | (_, Err(d)) => return Ok(Err(d)),
        };
        let rules = self.opened()?.rules.clone();
        let list = preset.reclist(&rules)?;
        let added = self.writer()?.add_subbank(&NewSubbank {
            voice_color: color.as_ref(),
            method: preset.method,
            preset_id: preset.id,
            tone,
            after: input.after,
            list: &list,
            rules: &rules,
        })?;
        let id = match added {
            Ok(id) => id,
            Err(r) => return Ok(Err(r.into())),
        };
        self.ensure_tone_dirs()?;
        self.subbank_summary(id)
    }

    /// 枠の声色・収録音階・方式プリセットを付け替える（SDL の `reviseSubbank`、`TR-RCL-30`）。
    ///
    /// 録っている途中は、収録音階と方式プリセットを変えない。 録っている行の音高が
    /// 確定の前に変わると、テイクが置かれる場所と行の音高が食い違う。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、録音リストを作れない、台帳かディレクトリを書けない。
    #[tracing::instrument(skip(self, input))]
    pub fn revise_subbank(
        &mut self,
        id: SubbankId,
        input: &ReviseSubbank<'_>,
    ) -> Result<SubbankAnswer<SubbankSummary>> {
        if input.clear_voice_color && input.voice_color.is_some() {
            return Ok(Err(invalid(
                "clearVoiceColor",
                "subbank.voice_color_and_clear",
            )));
        }
        let color = match input.voice_color.map(|c| voice_color("voiceColor", c)) {
            None => None,
            Some(Ok(c)) => Some(c),
            Some(Err(d)) => return Ok(Err(d)),
        };
        let new_tone = match input.tone.map(|t| tone("tone", t)).transpose() {
            Ok(t) => t,
            Err(d) => return Ok(Err(d)),
        };
        let new_preset = match input.preset.map(|p| preset("preset", p)).transpose() {
            Ok(p) => p,
            Err(d) => return Ok(Err(d)),
        };
        if (new_tone.is_some() || new_preset.is_some()) && self.recording.is_some() {
            return Ok(Err(SubbankRefusal::Recording.into()));
        }
        let rules = self.opened()?.rules.clone();
        let list = match new_preset {
            Some(p) => Some(p.reclist(&rules)?),
            None => None,
        };
        let change = SubbankRevision {
            voice_color: if input.clear_voice_color {
                Some(None)
            } else {
                color.as_ref().map(Some)
            },
            tone: new_tone,
            preset: new_preset
                .zip(list.as_deref())
                .map(|(p, list)| PresetChange {
                    preset_id: p.id,
                    method: p.method,
                    list,
                    rules: &rules,
                }),
        };
        if let Err(r) = self.writer()?.revise_subbank(id, &change)? {
            return Ok(Err(r.into()));
        }
        self.ensure_tone_dirs()?;
        self.subbank_summary(id)
    }

    /// 枠を並べ替える（SDL の `reorderSubbanks`）。 `order` はすべての枠を1回ずつ。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, order), fields(subbanks = order.len()))]
    pub fn reorder_subbanks(
        &mut self,
        order: &[SubbankId],
    ) -> Result<SubbankAnswer<Vec<SubbankSummary>>> {
        if let Err(r) = self.writer()?.reorder_subbanks(order)? {
            return Ok(Err(r.into()));
        }
        Ok(Ok(self.subbanks()?))
    }

    /// 枠の担当音域を直す（SDL の `setSubbankToneRange`、`TR-RCL-31`）。 `None` で既定へ戻す。
    ///
    /// 返すのは、直した枠と、同じ声色で担当音域が動いた枠。 重なりも隙間も弾かない
    /// （`Q-RCL-005`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, range))]
    pub fn set_subbank_tone_range(
        &mut self,
        id: SubbankId,
        range: Option<(i32, i32)>,
    ) -> Result<SubbankAnswer<Vec<SubbankSummary>>> {
        let range = match range
            .map(|(low, high)| ToneRange::edited(low, high))
            .transpose()
        {
            Ok(r) => r,
            Err(e) => return Ok(Err(SubbankDenied::Invalid(range_violation(e)))),
        };
        let before = self.subbanks()?;
        if let Err(r) = self.writer()?.set_subbank_tone_range(id, range)? {
            return Ok(Err(r.into()));
        }
        let Some(color) = before
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.voice_color.clone())
        else {
            return Ok(Err(SubbankRefusal::UnknownSubbank.into()));
        };
        Ok(Ok(self
            .subbanks()?
            .into_iter()
            .filter(|s| {
                s.id == id
                    || (s.voice_color == color
                        && before.iter().any(|b| {
                            b.id == s.id && (b.range, b.range_edited) != (s.range, s.range_edited)
                        }))
            })
            .collect()))
    }

    /// 枠を消す（SDL の `removeSubbank`、`TR-RCL-29`）。 その枠の行とテイクと音声が消え、
    /// ほかの枠は変わらない（`INV-RCL-102`）。
    ///
    /// 録っている途中は消さない。 消すのは台帳を確定してからで、消せなかったファイルは
    /// 記録して続ける——台帳が持たない WAV は、次の起動時検証に孤児として出る（`TR-REC-31`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, acknowledged_takes))]
    pub fn remove_subbank(
        &mut self,
        id: SubbankId,
        acknowledged_takes: usize,
    ) -> Result<SubbankAnswer<SubbankRemoval>> {
        if self.recording.is_some() {
            return Ok(Err(SubbankRefusal::Recording.into()));
        }
        let removed = match self.writer()?.remove_subbank(id, acknowledged_takes)? {
            Ok(r) => r,
            Err(r) => return Ok(Err(r.into())),
        };
        let root = self.opened()?.dir().root().to_path_buf();
        for rel in &removed.files {
            let wav = root.join(rel);
            if let Ok(frq) = koeru_core::frq::frq_path(&wav) {
                delete_take_file(&frq);
            }
            delete_take_file(&wav);
        }
        self.refresh_review()?;
        Ok(Ok(SubbankRemoval {
            subbank: id,
            removed_takes: removed.takes,
        }))
    }

    /// 台帳の版（SDL の `Revision`）。 版を読み直させる断り（`RevisionConflict`）に載せる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    pub fn ledger_revision(&mut self) -> Result<koeru_core::db::Revision> {
        Ok(self.writer()?.revision()?)
    }

    /// 1つの枠を、一覧と同じ形で。 足した・付け替えた直後に返す。
    fn subbank_summary(&mut self, id: SubbankId) -> Result<SubbankAnswer<SubbankSummary>> {
        Ok(self
            .subbanks()?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| SubbankRefusal::UnknownSubbank.into()))
    }

    /// 多音階になったら、音高ごとのサブディレクトリを揃える（`TR-REC-36`）。
    ///
    /// 枠を足して音階が2つになると、それまでの音階のテイクも次からはサブディレクトリへ
    /// 置く（`start_take_for`）。 作るときと同じく、録る前に全部作っておく。
    fn ensure_tone_dirs(&mut self) -> Result<()> {
        let tones = self.writer()?.recording_tones()?;
        if tones.len() > 1 {
            let audio = self.opened()?.dir().audio_dir();
            for t in &tones {
                std::fs::create_dir_all(audio.join(koeru_core::tone::name(*t)))?;
            }
        }
        Ok(())
    }
}

fn summary(
    s: &SubbankRecord,
    default: ToneRange,
    left: &[koeru_core::reclist::Row],
) -> SubbankSummary {
    SubbankSummary {
        id: s.id,
        voice_color: s.voice_color.clone(),
        method: s.method,
        preset_id: s.preset_id.clone(),
        tone: s.tone,
        range: s.edited_range.unwrap_or(default),
        range_edited: s.edited_range.is_some(),
        rows: s.rows,
        recorded_rows: s.recorded_rows,
        remaining_seconds: koeru_core::pace::fixed_seconds(s.method, left, 1),
        takes: s.takes,
    }
}

/// 消した枠のテイクのファイルを消す。 無ければそれでよい。
///
/// 消せなくても続ける。 台帳はもう確定しているので、残ったファイルは次回の起動時検証
/// （`TR-REC-31`）に孤児として出る。
fn delete_take_file(path: &std::path::Path) {
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            // パスは載せない（トレースのホワイトリスト方針、`AGENTS.md` の禁止事項3）。
            koeru_failure::record(
                "subbank.delete_file_failed",
                koeru_failure::io_class(&e),
                koeru_failure::Outcome::Committed,
                "subbank.delete_file",
            );
        }
    }
}
