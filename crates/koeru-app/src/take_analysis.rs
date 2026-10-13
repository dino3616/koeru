//! テイクの解析を「入力を集める／計算する／受理して書く」の3段に分けたときの、真ん中の段。
//!
//! 計算は [`Studio`](crate::studio::Studio) も台帳の書き手も引数に取らない
//! （`DEC-PLT-034`。計算の域は台帳へ書く権限を持たない）。 読むものは
//! [`TakeAnalysisInput`] が所有した値で持ち、置くものは [`TakeAnalysisOutput`] が
//! 置く順に並べて返す。 台帳とファイルへ置くのは `Studio::admit_take_analysis` だけ。
//!
//! 計算は `JobRuntime` の作業スレッドで走る（`DEC-PLT-047`、T07b-2）。 計算の中身も、
//! 置く順序も以前と同じ（`TR-ALN-29`）。 増えたのは、区切りで取消の合図を見る点だけ
//! ——F0 推定の前後と、アライメントの前後。 合図を見る関数は呼び出し側から受け取り
//! （[`Checkpoint`]）、`JobRuntime` の型はここへ持ち込まない。
//!
//! # 読んだ入力の印
//!
//! 計算が読んだ入力は [`analysis_based_on`] / [`re_estimate_based_on`] が名前と指紋に
//! して返す。 公開のときに `JobRuntime` が今の値と突き合わせ、違えば結果を捨てる
//! （`PublishRefused::Stale`）。 **印に載せるのは計算が読んだものだけ**——読んでいない
//! ものを入れると、無関係な編集で結果を無駄に捨てる。 指紋は中身を直接載せない
//! （音源名・歌詞・パスが記録へ流れ込まないよう、ハッシュにする）。
//!
//! # 台帳を読んだ失敗は値で持つ
//!
//! 台帳から読むのは入力を集める段で、読めなかったときも計算は止めない。 以前は読む場所
//! ごとに `?` で止まり、そこまでに置いたものは残った。 その意味を保つため、読みは
//! 失敗ごと [`RowReads`] に持ち、計算がその読みを要る段に着いたときに出力へ移す。
//!
//! # 出力は「置く順」を持つ
//!
//! [`TakeAnalysisOutput::steps`] は置く工程を並べたもので、受理する側は上から順に実行し、
//! 最初に落ちた工程で止まる。 計算が途中で落ちたら、その手前までを並べて、落ちた理由を
//! [`TakeAnalysisOutput::failure`] に入れる。

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use koeru_align::aligner::{AlignRequest, Alignment};
use koeru_align::confidence::{Confidence, from_alignment_span};
use koeru_align::consistency::{self, Measure};
use koeru_align::phoneme::Phoneme;
use koeru_align::preset::Preset;
use koeru_align::segment::{Boundaries, SegmentConfig, confidence, per_mora};
use koeru_core::alias::Method;
use koeru_core::analysis::{TakeAnalysis, TakeMetrics};
use koeru_core::db::{ConfidenceParts, FingerprintRow, GapRecord, GapRecordKind, TakeGapCounts};
use koeru_core::fingerprint::Canonical;
use koeru_core::inventory::{Unit, UnitSet};
use koeru_core::oto::{Boundary, Oto};
use koeru_core::presamp::Rules;
use koeru_core::reclist::{self, Slot};
use koeru_failure::Class;
use koeru_runtime::jobs::{BasedOn, Cancelled};
use koeru_synth::f0;

use crate::align::Chosen;
use crate::capture_lease::{TakeGap, TakeGapKind};
use crate::error::AppError;

/// 解析の期限（投入から）。 過ぎたら失敗にし、自動でやり直さない（`DEC-PLT-047`）。
///
/// **仮の値で、根拠が無い**（`Q-PLT-006`）。 以前は期限が無かった。 短すぎれば、
/// 正当な解析を失敗にする。 長ければ、固まったエンジンを見つけるのが遅れる。
/// 待ちの時間も含むので、長い解析の後ろに並んだものも同じ期限で数える。
pub(crate) const ANALYSIS_DEADLINE: Duration = Duration::from_secs(5 * 60);

/// 区切りで取消の合図を見る関数。 取り消されていれば `Err` で、計算はそこで抜ける。
///
/// `JobRuntime` の `CancelProbe::check` を渡す。 試験は `&|| Ok(())` を渡す。
pub(crate) type Checkpoint<'a> = &'a dyn Fn() -> Result<(), Cancelled>;

/// 計算が最後まで走らずに戻った理由。
#[derive(Debug)]
pub(crate) enum Stop {
    /// 区切りで取消の合図を見て抜けた。
    Cancelled(Cancelled),
    /// 計算が落ちた。
    Failed(AppError),
}

impl From<Cancelled> for Stop {
    fn from(c: Cancelled) -> Self {
        Self::Cancelled(c)
    }
}

impl From<AppError> for Stop {
    fn from(e: AppError) -> Self {
        Self::Failed(e)
    }
}

/// 並べた文字列の指紋（`DEC-PLT-053`）。 中身は記録へ流さず、同じ値かどうかだけが言える。
///
/// 内容の SHA-256 で、`domain` が用途を分ける。 並べ方は `koeru_model::fingerprint::Canonical`
/// が決めており、版や起動で値が変わらない。 **以前は `DefaultHasher` で、同じプロセスの中でしか
/// 比べられなかった**——受領証や永続化へ載せられる値ではなかった。
fn digest<'a>(domain: &str, parts: impl IntoIterator<Item = &'a str>) -> String {
    parts
        .into_iter()
        .fold(Canonical::new(domain), |c, part| c.text("part", part))
        .finish()
        .to_hex()
}

/// 読みが失敗していたら、失敗した code を指紋にする。 失敗を読んだことも入力の一部。
fn stamp_of<T>(read: &Result<T, AppError>, fingerprint: impl FnOnce(&T) -> String) -> String {
    match read {
        Ok(v) => fingerprint(v),
        Err(e) => format!("unreadable:{}", e.code),
    }
}

/// 読みの最初の音素。集団の鍵にする（`TR-ALN-12` (b)）。
///
/// 音素へ写せない読みは `None`。 推測で既定の音素を当てない——
/// 別の音素の集団に混ざると、その集団の中央値まで動く。
pub(crate) fn first_phoneme(reading: &str) -> Option<Phoneme> {
    koeru_align::phoneme::phonemes_for(reading)
        .ok()
        .and_then(|p| p.first().copied())
}

/// 読みの最後の音素。母音長の集団の鍵にする（`TR-ALN-12` (b)）。
///
/// 母音の長さは母音で決まる。 子音で束ねると、あ段とう段が同じ集団に入る。
pub(crate) fn last_phoneme(reading: &str) -> Option<Phoneme> {
    koeru_align::phoneme::phonemes_for(reading)
        .ok()
        .and_then(|p| p.last().copied())
}

/// 話者内一貫性（`TR-ALN-12`）の集団を引く読み。 CV の枠だけ返す。
///
/// 集団の鍵は音素で、音素は仮名から引く（`phoneme::phonemes_for` は仮名の辞書）。
/// **綴りを渡していた。** `a か` も `- か` も `a k` も辞書に無いので、
/// 連続音と CVVC では集団が1つも作れず、事前分布がいつも 1.0 だった
/// ——単独音だけは綴りが仮名そのものなので、試験が素通りしていた。
///
/// 渡りと語尾は測らない。 先行発声も子音長も CV とは別の区間を指すので、
/// 同じ音素の集団へ混ぜると CV の集団のほうが歪む。分からないものを
/// 外れ値にしない方針（[`prior_of`]）どおり、1.0 のままにする。
pub(crate) fn consistency_reading(slot: Slot, line: &[Unit]) -> Option<&'static str> {
    match slot {
        Slot::Cv { mora } => line.get(mora).map(|u| u.kana),
        Slot::Vc { .. } | Slot::Ending { .. } => None,
    }
}

/// 音素ごとに集めた測度の集団（`TR-ALN-12`）。
///
/// 条文が挙げるのは「先行発声位置・子音長・母音長」。 oto からそれぞれを引く。
///
/// **測度ごとに違う音素で集める。** 先頭音素ひとつで全部を束ねていたので、
/// か・く・け・こ の**母音長が全部 `k` の集団へ入り**、あ段の長さと
/// う段の長さが同じ集団で比べられていた。母音の長さは母音で決まる。
///
/// **子音長は先行発声位置の定数ずらしになる**（`derive_cv`: 子音長 =
/// 先行発声 − 前余白マージン）。同じプリセットの中では中央値からの距離が
/// 一致するので、別に測っても新しいことは言わない。それでも並べているのは、
/// プリセットが混ざったプロジェクト（`TR-ALN-23` の編集）でずれるため。
#[derive(Debug, Default)]
pub(crate) struct Populations {
    /// (収録音高, 先頭音素) → 先行発声位置と子音長。
    ///
    /// **音階を鍵に含める**（`TR-ALN-22` の「集団統計は音階内に閉じる」）。
    /// 高音階と低音階では発声が変わるので、混ぜると全部が外れ値になる。
    /// `consistency::Population` は初めから音階を持っていたが、
    /// ここの集計が音素だけで畳んでいた。**踏んだ。**
    onset: HashMap<(i32, Phoneme), (Vec<f64>, Vec<f64>)>,
    /// (収録音高, 末尾音素) → 母音長。
    vowel: HashMap<(i32, Phoneme), Vec<f64>>,
}

impl Populations {
    /// その oto から子音長と母音長を引く。
    ///
    /// 母音長 = 使える区間 − 先行発声。 子音長 = 先行発声（前余白ぶんずれている）。
    fn spans(o: &Oto, len_ms: f64) -> (f64, f64) {
        let usable = o.usable_ms(len_ms);
        (o.preutterance_ms, (usable - o.preutterance_ms).max(0.0))
    }

    pub(crate) fn push(&mut self, tone: i32, reading: &str, o: &Oto, len_ms: f64) {
        let (consonant, vowel) = Self::spans(o, len_ms);
        if let Some(k) = first_phoneme(reading) {
            let e = self.onset.entry((tone, k)).or_default();
            e.0.push(o.preutterance_ms);
            e.1.push(consonant);
        }
        if let Some(k) = last_phoneme(reading) {
            self.vowel.entry((tone, k)).or_default().push(vowel);
        }
    }
}

/// 集団から見た、その推定の素直さ（`TR-ALN-12`）。
///
/// 1.0 が「集団の真ん中」、0.0 が「大きく外れている」。
/// 集団が集まっていなければ 1.0——序盤のテイクを外れ値にしない
/// （`TR-ALN-10` notes）。
///
/// **3つの測度のうち、いちばん外れているものが決める。** どれか1つでも
/// 集団から外れていれば見てほしいので、平均では薄まる。
/// `tone` はそのテイクの収録音高（`TR-ALN-22`）。 集団は音階内に閉じる。
fn prior_of(pops: &Populations, tone: i32, reading: &str, o: &Oto, len_ms: f64) -> f64 {
    let score = |measure, value: f64, population: &[f64]| {
        consistency::deviation(measure, value, population).map_or(1.0, |d| d.prior_score())
    };
    let (consonant, vowel) = Populations::spans(o, len_ms);
    // 音素へ写せない読みは集団を作れない。 分からないものを外れ値にしない。
    let onset = first_phoneme(reading)
        .and_then(|k| pops.onset.get(&(tone, k)))
        .map_or(1.0, |(pre, cons)| {
            score(Measure::Preutterance, o.preutterance_ms, pre).min(score(
                Measure::ConsonantLength,
                consonant,
                cons,
            ))
        });
    let tail = last_phoneme(reading)
        .and_then(|k| pops.vowel.get(&(tone, k)))
        .map_or(1.0, |v| score(Measure::VowelLength, vowel, v));
    onset.min(tail)
}

/// この音源の作り方と、綴りの表。 行を問わない。
#[derive(Debug)]
pub(crate) struct Setup {
    /// 作り方（`TR-RCL-01`）。 原音設定の規約がこれで決まる。
    pub method: Method,
    /// 音素インベントリのセット。 行の単位を引くのに要る。
    pub set: UnitSet,
    /// 綴りの表（`TR-SYN-36`）。 録音リストを入れたときと同じものを引く。
    pub rules: Rules,
}

/// 台帳から読んだ、1行についての事実。 どれも、読めなければ失敗を持つ。
///
/// 読む順は、以前の `?` で止まる順と同じ。 計算はその順に取り出す。
#[derive(Debug)]
pub(crate) struct RowReads {
    pub setup: Result<Setup, AppError>,
    /// 行の読み。 読み上げる順（`Ledger::units_of`）。
    pub kana: Result<Vec<String>, AppError>,
    /// 行の収録単位。 綴りを作るのに子音と母音クラスが要る。 並びは読み上げ順
    /// （`Ledger::row_units_of`）。
    pub line: Result<Vec<Unit>, AppError>,
    /// 集団は行の中で変わらない。1度だけ作る（`TR-ALN-12`）。
    pub populations: Result<Populations, AppError>,
    /// その行の収録音高。 集団は音階内に閉じる（`TR-ALN-22`）。
    pub tone: Result<i32, AppError>,
    /// 5値を置くのは、この行が持ち主の綴りだけ（`TR-ALN-22`, `DEC-RCL-016`）。
    pub owned: Result<BTreeSet<String>, AppError>,
}

/// 読みに失敗が入っていたら、その失敗を返す。
fn loaded<T>(read: &Result<T, AppError>) -> Result<&T, AppError> {
    read.as_ref().map_err(Clone::clone)
}

impl RowReads {
    /// 計算が読んだ行の事実の印。 `based` へ足して返す。
    ///
    /// **集団（`populations`）は載せない。** 他のテイクの oto から集めた統計で、
    /// 確信度の成分 (3) にしか効かない。 変わっても、置く値の正しさ（持ち主の綴り・
    /// 境界・oto の組）は崩れない。 載せると、無関係なテイクの確定のたびに結果を捨てる。
    /// 持ち主（`owned`）は載せる——先に録った行が取れば、同じ綴りを二重に置く
    /// （`DEC-RCL-016`）。
    fn stamp(&self, based: BasedOn) -> BasedOn {
        based
            .with(
                "row.setup",
                stamp_of(&self.setup, |s| {
                    // 規則は綴りの表の写しの文字列にして引く。 順序が決まっている。
                    digest(
                        "row.setup",
                        [
                            format!("{:?}", s.method).as_str(),
                            format!("{:?}", s.set).as_str(),
                            koeru_core::presamp::write(&s.rules, "\n").as_str(),
                        ],
                    )
                }),
            )
            .with(
                "row.kana",
                stamp_of(&self.kana, |k| {
                    digest("row.kana", k.iter().map(String::as_str))
                }),
            )
            .with(
                "row.line",
                stamp_of(&self.line, |l| {
                    digest("row.line", [format!("{l:?}").as_str()])
                }),
            )
            .with("row.tone", stamp_of(&self.tone, |t| t.to_string()))
            .with(
                "row.owned",
                stamp_of(&self.owned, |o| {
                    digest("row.owned", o.iter().map(String::as_str))
                }),
            )
    }
}

/// F0 の条件の印。 計算が読んだのは、集めた数そのものではなく、そこから決めた条件
/// （[`f0::Purpose`] と下限）。
fn f0_stamp(based: BasedOn, purpose: f0::Purpose, floor: Option<f64>) -> BasedOn {
    let purpose = match purpose {
        f0::Purpose::Preview => "preview",
        f0::Purpose::Distribution => "distribution",
    };
    based.with("f0.purpose", purpose).with(
        "f0.floor",
        floor.map_or_else(|| "none".to_owned(), |f| format!("{:016x}", f.to_bits())),
    )
}

/// テイクの解析が読んだ入力の印。
///
/// 受理の直前に、同じ引数を今の値で作り直して突き合わせる。 引数が同じなら、印は同じ名前を
/// 同じ作り方で作る——これが1つの関数にあることで、「確定のときの印」と「受理のときの今の値」が
/// ずれない。
pub(crate) fn analysis_based_on(
    take_id: i32,
    purpose: f0::Purpose,
    floor: Option<f64>,
    row: &RowReads,
) -> BasedOn {
    row.stamp(f0_stamp(
        BasedOn::new().with("take", take_id.to_string()),
        purpose,
        floor,
    ))
}

/// 推定し直しが読んだ入力の印。 `wav` はテイクの WAV の指紋（読み直した標本が
/// 別のファイルのものになっていないか）。
pub(crate) fn re_estimate_based_on(take_id: i32, wav: &str, row: &RowReads) -> BasedOn {
    row.stamp(
        BasedOn::new()
            .with("take", take_id.to_string())
            .with("take.wav", wav),
    )
}

/// テイクの派生物を作るのに要る、確定のときに分かっていたこと。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TakeFacts {
    /// 取りこぼし・不連続・レンダの失敗の数（`TR-REC-07`）。
    pub counts: TakeGapCounts,
    /// 記録できた欠落の位置。 テイクの先頭からのマスター標本位置。
    pub gaps: Vec<TakeGap>,
    pub guide_offset: Option<i64>,
    /// 押した瞬間より前から遡れたフレーム数（`TR-REC-19`）。
    pub preroll_frames: usize,
    pub duration_ms: f64,
}

/// テイクの解析が読むもの全部。 所有した、変わらない値で持つ。
///
/// 音域の統計（`ProjectCaches::observed_f0` と `f0_floor`）は、集めた時点の写しを
/// 持つだけで、計算の中では更新しない。 更新は受理する側がする。
#[derive(Debug)]
pub(crate) struct TakeAnalysisInput {
    pub take_id: i32,
    pub row_id: String,
    /// 確定した WAV。 `.frq` の書き先を決めるのに要る。
    pub wav_path: PathBuf,
    /// マスターの標本。 **`Arc<Vec<f32>>` で持つ。** `Arc<[f32]>::from` は複写する。
    pub samples: Arc<Vec<f32>>,
    pub rate: u32,
    /// F0 の条件（`TR-SYN-22`）。 音域を集めた数から決める。
    pub f0_purpose: f0::Purpose,
    pub f0_floor: Option<f64>,
    pub facts: TakeFacts,
    pub row: RowReads,
    pub aligner: Chosen,
}

impl TakeAnalysisInput {
    /// 計算が読んだ入力の印（[`analysis_based_on`]）。
    pub(crate) fn based_on(&self) -> BasedOn {
        analysis_based_on(self.take_id, self.f0_purpose, self.f0_floor, &self.row)
    }

    /// 入力の大きさ（バイト）。 `JobRuntime` の上限の勘定に申告する。
    pub(crate) fn input_bytes(&self) -> usize {
        self.samples
            .len()
            .saturating_mul(std::mem::size_of::<f32>())
    }
}

/// 計測値（`TR-REC-16`）と、欠落の記録（`TR-REC-07`）。
#[derive(Debug)]
pub(crate) struct Measured {
    pub metrics: TakeMetrics,
    pub counts: TakeGapCounts,
    pub preroll_frames: usize,
    pub guide_offset: Option<i64>,
    /// 欠落の位置（`TR-REC-07` の「欠落の発生数と位置をメタデータに記録する」）。
    pub gaps: Vec<GapRecord>,
}

/// 台帳へ置く oto 1つ。
#[derive(Debug)]
pub(crate) struct EntryRow {
    pub alias: String,
    pub oto: Oto,
    /// 合成スコア。
    pub score: f64,
    /// 成分も残す（`TR-ALN-24`）。 合成からは作り直せない。
    pub parts: ConfidenceParts,
    /// 無声破裂音の分岐が閉鎖で確かめられなかった（`TR-ALN-16`, `DEC-ALN-018`）。
    pub mismatch: bool,
}

/// 5値と、何で推定したかの札。
#[derive(Debug)]
pub(crate) struct Entries {
    pub rows: Vec<EntryRow>,
    /// 何で推定したかを残す（`TR-ALN-29`）。 モデルが変わったときに、黙って作り直さない
    /// ための鍵。
    pub fingerprint: FingerprintRow,
    /// 画面へ返す、先頭の oto と確信度。 テイク全体の確信度は先頭モーラのものを出す。
    pub first: Option<(Oto, f64)>,
}

/// 置く工程。 [`TakeAnalysisOutput::steps`] に置く順に並ぶ。
#[derive(Debug)]
pub(crate) enum Step {
    /// 解析値と `.frq`（`TR-PKG-05`）。 `.frq` は素材ファイル全体を渡す（`DEC-SYN-008`）。
    Analysis,
    /// アライナが揃えられなかった。 台帳には何も置かず、理由だけを記録する。
    ///
    /// 原音設定を作らないだけで、テイクの確定は取り消さない。 なぜ揃わなかったかは
    /// code で残す。
    AlignmentFailed(AppError),
    Measured(Box<Measured>),
    /// 境界（`TR-ALN-34`）。 5値は境界と規約プリセットから導く派生物なので、
    /// 境界さえあれば規約を変えても作り直せる。
    Boundaries(Vec<(String, Boundary)>),
    Entries(Box<Entries>),
    /// 確認キューへ入れる（`REQ-ALN-002`）。
    Enqueue,
}

/// [`analyze_take`] の出力。 台帳とファイルへ置くものを全部持つ。
#[derive(Debug)]
pub(crate) struct TakeAnalysisOutput {
    pub take_id: i32,
    pub wav_path: PathBuf,
    /// 今回の F0 系列。 受理する側が音域へ足す。
    pub f0: Vec<f64>,
    pub analysis: TakeAnalysis,
    /// 境界が分かる前の測り方。 揃えられれば [`Step::Measured`] で測り直す。
    pub measured: TakeMetrics,
    pub steps: Vec<Step>,
    /// 計算が途中で落ちたときの失敗。 `steps` はその手前までしか持たない。
    pub failure: Option<AppError>,
}

/// テイクの解析を計算する。 `Studio` も台帳の書き手も取らない。
///
/// 計算の失敗は返さない。 最初に落ちた工程の失敗を [`TakeAnalysisOutput::failure`] に入れ、
/// そこまでに作れたものを返す。 返す `Err` は、区切りで取り消しの合図を見て抜けたときだけ。
///
/// # Errors
///
/// `checkpoint` が取り消しを返した（F0 推定の前後、アライメントの前後）。
pub(crate) fn analyze_take(
    input: &TakeAnalysisInput,
    checkpoint: Checkpoint<'_>,
) -> Result<TakeAnalysisOutput, Cancelled> {
    let rate = input.rate;
    checkpoint()?;

    // ## 解析。録音停止時に確定させて、以後 WAV を読み直さない
    let f64s: Vec<f64> = input.samples.iter().map(|s| f64::from(*s)).collect();
    // 試唱のために走らせる解析を、そのまま .frq へ回す（`TR-PKG-05`）。
    // 書き出しのために推定し直さない。
    //
    // 二段構え（`TR-SYN-22`）。最初の数テイクは DIO+StoneMask で即座に確定し、
    // 話者音域が判明したら Harvest で引き直す。
    // `.frq` が要求するのは F0 と平均振幅だけなので、初期テイクの試唱には
    // DIO の精度で足りる。待たせないことのほうが効く。
    let cond = f0::conditions(input.f0_purpose, input.f0_floor);
    let (source_f0, _t) = f0::estimate(&f64s, rate, &cond);
    checkpoint()?;
    let analysis = TakeAnalysis::compute(
        &input.samples,
        rate,
        &source_f0,
        cond.frame_period_ms / 1000.0,
    );
    // 境界が分かる前の測り方。 揃えられれば下で測り直す。
    let measured = TakeMetrics::measure(&input.samples, rate, None, None);

    let mut steps = Vec::new();
    let failure = match plan(input, &f64s, &mut steps, checkpoint) {
        Ok(()) => None,
        Err(Stop::Failed(e)) => Some(e),
        Err(Stop::Cancelled(c)) => return Err(c),
    };
    Ok(TakeAnalysisOutput {
        take_id: input.take_id,
        wav_path: input.wav_path.clone(),
        f0: source_f0,
        analysis,
        measured,
        steps,
        failure,
    })
}

/// [`analyze_take`] のうち、台帳とファイルへ置く工程を並べる。 作れた順に `steps` へ足す。
fn plan(
    input: &TakeAnalysisInput,
    f64s: &[f64],
    steps: &mut Vec<Step>,
    checkpoint: Checkpoint<'_>,
) -> Result<(), Stop> {
    let rate = input.rate;
    let facts = &input.facts;
    // この音源の作り方（`TR-RCL-01`）。 原音設定の規約がこれで決まる。
    let setup = loaded(&input.row.setup)?;

    steps.push(Step::Analysis);

    // ## 境界と oto
    //
    // アライナを通す（`TR-ALN-03`）。 揃えられなければ原音設定を作らないだけで、
    // テイクの確定は取り消さない。 なぜ揃わなかったかは code で残す。
    checkpoint()?;
    let alignment = match loaded(&input.row.kana)
        .and_then(|kana| align_samples(&input.aligner, kana, f64s, rate))
    {
        Ok(a) => Some(a),
        Err(e) => {
            steps.push(Step::AlignmentFailed(e));
            None
        }
    };
    checkpoint()?;
    // 1ファイルに複数モーラが入る（`TR-RCL-03`、`DEC-ALN-013`）。
    // モーラごとに境界を取り出し、oto もモーラごとに作る。
    let kana = loaded(&input.row.kana)?;
    let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
    // 綴りを作るのに子音と母音クラスが要る。 並びは読み上げ順
    // （`Ledger::row_units_of`）。
    let line = loaded(&input.row.line)?;
    let per_mora = alignment.as_ref().and_then(|a| per_mora(a, &readings));
    // 計測（`TR-REC-16`）と無音マージン（`TR-REC-38`）はファイル全体で見る。
    let boundaries = per_mora.as_ref().and_then(|v| {
        Some(Boundaries {
            voice_start_ms: v.first()?.voice_start_ms,
            vowel_start_ms: v.first()?.vowel_start_ms,
            vowel_end_ms: v.last()?.vowel_end_ms,
        })
    });

    // ## 計測（`TR-REC-16`）と無音マージン（`TR-REC-38`）
    // 測るだけ。判定も指摘もしない。
    let metrics = TakeMetrics::measure(
        &input.samples,
        rate,
        boundaries.as_ref().map(|b| b.voice_start_ms),
        boundaries.as_ref().map(|b| b.vowel_end_ms),
    );
    let gaps: Vec<GapRecord> = facts
        .gaps
        .iter()
        .map(|g| GapRecord {
            kind: match g.kind {
                TakeGapKind::Dropped => GapRecordKind::Dropped,
                TakeGapKind::Discontinuity => GapRecordKind::Discontinuity,
                TakeGapKind::RenderError => GapRecordKind::RenderError,
            },
            position: i64::try_from(g.position).unwrap_or(i64::MAX),
        })
        .collect();
    steps.push(Step::Measured(Box::new(Measured {
        metrics,
        counts: facts.counts,
        preroll_frames: facts.preroll_frames,
        guide_offset: facts.guide_offset,
        gaps,
    })));

    if let Some(v) = per_mora {
        // 確信度はテイク全体で1つ。 事後確率があればそこから組み立てる
        // （`TR-ALN-24` の成分 (1)(2)）。無ければ退避経路の計算へ落ちる。
        // MFA が動いたのにパワー比で境界鋭さを測ると、
        // 要件の定義と違うものを記録することになる。
        // 確信度はモーラごとに作る（`TR-ALN-26`）。
        //
        // **ファイル全体で1つ作ってエントリ全部へ写さない。** 境界鋭さは
        // いちばん弱い境界で決まるので、1モーラが曖昧なだけで全部の
        // 確信度と主因が同じ値になり、**どのエントリを見ればよいかが消える。**
        // 音響異常度も同じで、範囲外の割れが混ざる。

        // この音源の作り方の規約で導く（`TR-ALN-13`）。 単独音の規約で
        // 連続音の行を切ると、子音の扱いも母音の終端も別の位置になる。
        let preset = Preset::default_for(setup.method)
            .map_err(|e| AppError::from_failure(e).saying("規約プリセットを読めない"))?;
        // 集団は行の中で変わらない。1度だけ作る（`TR-ALN-12`）。
        let pops = loaded(&input.row.populations)?;
        // 集団は音階内に閉じる（`TR-ALN-22`）。
        let row_tone = *loaded(&input.row.tone)?;

        // 行が生むエイリアスと、その5値の出どころ（`TR-RCL-18`）。
        //
        // **モーラと1対1で置いていた。** 連続音では綴りが仮名のまま
        // 配られ、CVVC では渡りも語尾も `oto.ini` に入らなかった。
        let entries = reclist::row_entries(&setup.rules, setup.method, line);
        // 境界を残す（`TR-ALN-34`）。 5値は境界と規約プリセットから導く
        // 派生物なので、境界さえあれば規約を変えても作り直せる。
        // **捨てていた。** プリセットを編集するだけで再アライメントが
        // 走っていた——`TR-ALN-23` の「再アライメントを要求しない」と
        // 食い違ったまま出荷されていた（`DEC-ALN-014`）。
        //
        // 置く名前はそのモーラの CV エイリアス。 モーラの添字を
        // 名前にしない——下位方式へ書き出すとき（`TR-PKG-24`）に
        // 引き直すのは、この行の CV の綴りからになる。
        let saved: Vec<(String, Boundary)> = entries
            .iter()
            .filter_map(|(a, slot)| match *slot {
                Slot::Cv { mora } => Some((a.clone(), *v.get(mora)?)),
                _ => None,
            })
            .collect();
        steps.push(Step::Boundaries(saved));

        // 5値を置くのは、この行が持ち主の綴りだけ（`TR-ALN-22`, `DEC-RCL-016`）。
        // 先に録った行が持つので、同じ音高で同じ綴りを既に録ってあれば
        // ここでは置かない。
        //
        // **境界のほうは全モーラぶん残す。** 下位方式への書き出しは
        // モーラ順に境界を並べ直し、1つでも欠けたらその素材を丸ごと
        // 落とす（`packaging::rederived_entries`）。持ち主でない
        // 行頭の境界まで捨てると、その行の綴りが全部消える。
        let owned = loaded(&input.row.owned)?;
        let estimated = estimate_entries(&EstimateArgs {
            entries: &entries,
            owned,
            per_mora: &v,
            line,
            preset: &preset,
            method: setup.method,
            pops,
            tone: row_tone,
            alignment: alignment.as_ref(),
            f64s,
            rate,
            duration_ms: facts.duration_ms,
        });
        if !estimated.mismatches.is_empty() {
            tracing::info!(
                count = estimated.mismatches.len(),
                "閉鎖を確かめられなかった"
            );
        }
        let rows: Vec<EntryRow> = estimated
            .entries
            .iter()
            .map(|e| EntryRow {
                alias: e.alias.clone(),
                oto: e.oto,
                score: e.confidence.score(),
                parts: ConfidenceParts {
                    path: e.confidence.path,
                    sharpness: e.confidence.sharpness,
                    prior: e.confidence.prior,
                    acoustic: e.confidence.acoustic,
                },
                mismatch: estimated.mismatches.contains(&e.alias),
            })
            .collect();
        // テイク全体の確信度は先頭モーラのものを出す。 成分 (3) は
        // モーラごとに違うので、テイクに1つしかない値としては代表を採る。
        let first = estimated
            .entries
            .first()
            .map(|e| (e.oto, e.confidence.score()));

        // 何で推定したかを残す（`TR-ALN-29`）。
        // モデルが変わったときに、黙って作り直さないための鍵。
        let fp = koeru_align::determinism::Fingerprint::new(
            f64s,
            &kana.join(" "),
            &preset,
            input.aligner.identity(),
        );
        steps.push(Step::Entries(Box::new(Entries {
            rows,
            fingerprint: FingerprintRow {
                audio: fp.audio,
                reading: fp.reading,
                preset: fp.preset,
                aligner: fp.aligner,
            },
            first,
        })));
    }

    // ## 確認キュー
    //
    // 採否は確定と同じ一手で済んでいる（`Ledger::commit_take`）。 取りこぼしたテイクは
    // 採用の候補に入らないので、キューにも入れない。
    if facts.counts.discontinuities == 0 {
        steps.push(Step::Enqueue);
    }
    Ok(())
}

/// 1テイクをアライメントする（`TR-ALN-03`, `TR-ALN-10`）。
///
/// 録音完了の直後に呼ぶ（`TR-ALN-10` の逐次推定）。
///
/// `Alignment` をそのまま返す。 境界だけに畳んで返すと、
/// 事後確率が捨てられて `TR-ALN-24` の成分 (1) 経路確信度が永久に出せない
/// （一度そう書いた）。呼び出し側が確信度を組み立てるのに要る。
///
/// アライナが答えられなければ失敗を返す。 oto は付かず、エントリは未推定のまま
/// 確認キューへ回る。黙って諦めない——落ちた理由は code で残る。
///
/// **発声区間の検出で境界を作っていた。** `Chosen::detect` から退避経路を
/// 消したあとも、ここに2本残っていた——読みを音素へ写せないときと、
/// テキスト逸脱以外で MFA が落ちたとき。どちらも無音だけの区間を作り、
/// そこから5値を導いて、アライナを通していない値を推定済みとして
/// 出していた（`DEC-ALN-016`）。
///
/// **失敗を `None` に潰していた。** 読みを音素へ写せないことも、推定が通らないことも、
/// 台帳を読めないことも、呼び出し側には「発声が無い」としか見えなかった。
/// 失敗は code ごと返し、続けるかどうかは呼び出し側が決める。
///
/// テキスト逸脱も、それ以外の推定の失敗も、原音設定を作らない点では同じ扱い
/// （`TR-ALN-09`）。 違うのは記録に残る code だけ。
pub(crate) fn align_samples(
    aligner: &Chosen,
    kana: &[String],
    samples: &[f64],
    rate: u32,
) -> Result<Alignment, AppError> {
    // 読みは録音リストが持っている（`TR-ALN-07`。実行時に g2p を持ち込まない）。
    let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
    let phonemes =
        koeru_align::phoneme::phonemes_for_all(&readings).map_err(AppError::from_failure)?;

    let req = AlignRequest {
        samples,
        sample_rate_hz: rate,
        phonemes: &phonemes,
        grid: None,
    };
    aligner.align(&req).map_err(AppError::from_failure)
}

/// 行の5値と確信度を、境界から導くのに要るもの。
struct EstimateArgs<'a> {
    /// 行が生むエイリアスと出どころ（`TR-RCL-18`）。
    entries: &'a [(String, Slot)],
    owned: &'a BTreeSet<String>,
    per_mora: &'a [Boundaries],
    line: &'a [Unit],
    preset: &'a Preset,
    method: Method,
    pops: &'a Populations,
    tone: i32,
    /// 揃えられなかったときは `None`。 確信度は退避経路の計算で測る。
    alignment: Option<&'a Alignment>,
    f64s: &'a [f64],
    rate: u32,
    duration_ms: f64,
}

/// エイリアス1つぶんの推定。
#[derive(Debug)]
pub(crate) struct EntryEstimate {
    pub alias: String,
    pub oto: Oto,
    pub confidence: Confidence,
}

struct Estimated {
    /// 持ち主のエイリアスだけ。 `entries` の並びのまま。
    entries: Vec<EntryEstimate>,
    /// 閉鎖の短時間パワーで確かめられなかった分岐（`TR-ALN-16`, `DEC-ALN-018`）。
    mismatches: BTreeSet<String>,
}

/// 5値と確信度を導く。 テイクの確定と推定し直しが同じ式を通る。
fn estimate_entries(a: &EstimateArgs<'_>) -> Estimated {
    let derived: BTreeMap<String, Oto> =
        koeru_align::derive::derive_row(a.entries, a.per_mora, a.line, a.duration_ms, a.preset)
            .into_iter()
            .collect();
    // 無声破裂音の分岐を閉鎖の短時間パワーで確かめる（`TR-ALN-16`, `DEC-ALN-018`）。
    let mismatches = koeru_align::derive::closure_mismatches(
        a.entries, a.per_mora, a.line, a.f64s, a.rate, a.method, a.preset,
    );
    let cfg = SegmentConfig::default();
    let mut entries = Vec::new();
    for (alias, slot) in a.entries {
        if !a.owned.contains(alias) {
            continue;
        }
        let Some(oto) = derived.get(alias).copied() else {
            continue;
        };
        // 確信度は、その枠が乗っているモーラの区間で測る（`TR-ALN-26`）。
        // 渡りは直前のモーラの尾に乗るので、そちらを見る。
        let Some(b) = a.per_mora.get(slot.mora()) else {
            continue;
        };
        // 集団はそのモーラの仮名で引く（`consistency_reading`）。
        // 綴りは音素へ写せないので、渡すと集団が引けない。
        //
        // 話者内一貫性（`TR-ALN-12`）。 成分 (3) を差し替える。
        // `from_alignment` は 1.0 を置いて「呼び出し側が集団を持ったときに
        // 差し替える」と書いている。**合成スコアに掛けない**——掛けると
        // 成分としては 1.0 のまま残り、保存した内訳が嘘になる。
        // 集団が `MIN_SAMPLES` に満たなければ 1.0 のまま（`TR-ALN-10` notes）。
        let prior = consistency_reading(*slot, a.line)
            .map_or(1.0, |k| prior_of(a.pops, a.tone, k, &oto, a.duration_ms));
        let mut c = span_confidence(a.alignment, a.f64s, a.rate, b, &cfg);
        c.prior = prior;
        entries.push(EntryEstimate {
            alias: alias.clone(),
            oto,
            confidence: c,
        });
    }
    Estimated {
        entries,
        mismatches,
    }
}

/// 1モーラの区間で測った確信度（`TR-ALN-26`）。
///
/// 事後確率があればそこから、無ければ退避経路で測る。 区間は発声の始まりから母音の終わりまで。
///
/// **ファイル全体を渡していた。** 隣のモーラが割れていたり小さかったりするだけで、
/// きれいな枠まで確認待ちへ戻る——テイク確定側は区間で切っているので、固定を解いただけで
/// 別の答えが出ていた。
fn span_confidence(
    alignment: Option<&Alignment>,
    f64s: &[f64],
    rate: u32,
    b: &Boundaries,
    cfg: &SegmentConfig,
) -> Confidence {
    let (from_ms, to_ms) = (b.voice_start_ms, b.vowel_end_ms);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "位置はミリ秒。標本数へ落として範囲に丸める"
    )]
    let cut = |ms: f64| ((ms / 1000.0 * f64::from(rate)).max(0.0) as usize).min(f64s.len());
    let (a0, a1) = (cut(from_ms), cut(to_ms));
    let part = &f64s[a0.min(a1)..a1.max(a0)];
    alignment
        .and_then(|a| from_alignment_span(a, part, from_ms, to_ms))
        .unwrap_or_else(|| {
            // 退避経路もモーラの範囲で測る（`TR-ALN-26`）。
            // **ファイル全体の境界で測っていた。** MFA が無い環境
            //（書いていない OS、モデルが無いビルド）では、
            // 1モーラの曖昧さが全エントリへ伝播したままだった。
            //
            // 境界は切った先頭からの相対へ直す。 `segment::confidence`
            // はサンプルと同じ原点で位置を数える。
            let shifted = Boundaries {
                voice_start_ms: b.voice_start_ms - from_ms,
                vowel_start_ms: b.vowel_start_ms - from_ms,
                vowel_end_ms: b.vowel_end_ms - from_ms,
            };
            confidence(part, rate, &shifted, cfg)
        })
}

/// 推定し直しが読むもの全部（`REQ-ALN-007`, `TR-ALN-29`）。
#[derive(Debug)]
pub(crate) struct ReEstimateInput {
    pub take_id: i32,
    /// テイクの行。 受理の直前に、読んだ入力の今の値を作り直すのに要る。
    pub row_id: String,
    /// テイクの WAV の、プロジェクトの中での場所。 同上（WAV の指紋を引き直す）。
    pub rel_path: String,
    /// 台帳が持つテイクの WAV を読み直した標本。 レートは WAV のもの。
    pub samples: Arc<Vec<f32>>,
    pub rate: u32,
    pub row: RowReads,
    pub aligner: Chosen,
    /// テイクの WAV の指紋。 読んだ標本が別のファイルのものになっていないかを、印で見る。
    pub wav: String,
}

impl ReEstimateInput {
    /// 計算が読んだ入力の印（[`re_estimate_based_on`]）。
    pub(crate) fn based_on(&self) -> BasedOn {
        re_estimate_based_on(self.take_id, &self.wav, &self.row)
    }

    /// 入力の大きさ（バイト）。 `JobRuntime` の上限の勘定に申告する。
    pub(crate) fn input_bytes(&self) -> usize {
        self.samples
            .len()
            .saturating_mul(std::mem::size_of::<f32>())
    }
}

/// 推定し直した結果。
#[derive(Debug)]
pub(crate) struct ReEstimation {
    pub take_id: i32,
    /// その行の収録音高。 キューの鍵は（音高, 綴り）（`TR-ALN-22`）。
    pub tone: i32,
    pub entries: Vec<EntryEstimate>,
    pub mismatches: BTreeSet<String>,
}

/// そのテイクの oto を作り直す（`REQ-ALN-007`, `TR-ALN-29`）。
///
/// 台帳へは何も書かない。 固定されていない値だけを書き換える（`INV-ALN-001`）のは、
/// 結果を受け取ったキューの側。
///
/// 本人が推定し直しを求めたので、揃えられなければその理由を返す。
///
/// # Errors
///
/// 読みを読めない、読みを音素へ写せない、アライナが答えられない、発声を見つけられない、
/// 規約や行の事実を読めなかった（[`Stop::Failed`]）。 `checkpoint` が取り消しを返した
/// （アライメントの前後。[`Stop::Cancelled`]）。
pub(crate) fn re_estimate(
    input: &ReEstimateInput,
    checkpoint: Checkpoint<'_>,
) -> Result<ReEstimation, Stop> {
    let rate = input.rate;
    let f64s: Vec<f64> = input.samples.iter().map(|v| f64::from(*v)).collect();
    #[allow(
        clippy::cast_precision_loss,
        reason = "収録の長さは 2^53 サンプルに届かない"
    )]
    let duration_ms = f64s.len() as f64 * 1000.0 / f64::from(rate);

    let kana = loaded(&input.row.kana)?;
    let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
    checkpoint()?;
    let alignment = align_samples(&input.aligner, kana, &f64s, rate)?;
    checkpoint()?;
    let per_mora = per_mora(&alignment, &readings).ok_or_else(|| {
        AppError::new(
            "align.no_voice",
            Class::EngineFailed,
            "発声を見つけられなかった",
        )
    })?;

    // この音源の作り方の規約で導く（`TR-ALN-13`）。
    let setup = loaded(&input.row.setup)?;
    let preset = Preset::default_for(setup.method)
        .map_err(|e| AppError::from_failure(e).saying("規約プリセットを読めない"))?;
    let line = loaded(&input.row.line)?;
    let pops = loaded(&input.row.populations)?;
    // 集団は音階内に閉じる（`TR-ALN-22`）。
    let tone = *loaded(&input.row.tone)?;

    // 行が生むエイリアスごとに1つ（`TR-RCL-18`）。 確定のときと同じ表を通す
    // ——別の表で作り直すと、確認キューに無いエイリアスが生える。
    let entries = reclist::row_entries(&setup.rules, setup.method, line);
    // 持ち主の綴りだけ。 確定のときと同じ絞り方を通す（`TR-ALN-22`, `DEC-RCL-016`）。
    let owned = loaded(&input.row.owned)?;
    // 境界が動いたので、閉鎖も確かめ直す（`DEC-ALN-018`）。 前の印を残さない。
    let estimated = estimate_entries(&EstimateArgs {
        entries: &entries,
        owned,
        per_mora: &per_mora,
        line,
        preset: &preset,
        method: setup.method,
        pops,
        tone,
        alignment: Some(&alignment),
        f64s: &f64s,
        rate,
        duration_ms,
    });
    Ok(ReEstimation {
        take_id: input.take_id,
        tone,
        entries: estimated.entries,
        mismatches: estimated.mismatches,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    /// 合成音。 モーラごとに「雑音の子音 + 倍音のある母音」を並べる。 乱数は使わない。
    ///
    /// **構造しか見られない。** 合成音ではアライナの境界が正しい位置に出るかは言えない
    /// ——位置が全部ずれていても落ちない（`EVID-ALN-001`）。 実音声での確かめは
    /// `verify-koeru` の手順にある。
    pub(crate) fn synthetic_voice(moras: usize, f0_hz: f64) -> Vec<f32> {
        let rate = f64::from(koeru_audio::wav::MASTER_RATE_HZ);
        let silence = (0.4 * rate) as usize;
        let mut seed: u32 = 12345;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / 8_388_608.0 - 1.0
        };
        let mut out = vec![0.0_f32; silence];
        for m in 0..moras {
            let consonant = (0.10 * rate) as usize;
            for i in 0..consonant {
                out.push(noise() * (i as f32 / consonant as f32).min(1.0) * 0.15);
            }
            let vowel = (0.28 * rate) as usize;
            let f = f0_hz * (1.0 + 0.02 * m as f64);
            for i in 0..vowel {
                let t = i as f64 / rate;
                let env = (i as f64 / vowel as f64 * std::f64::consts::PI).sin();
                let v: f64 = (1..=6)
                    .map(|h| {
                        (2.0 * std::f64::consts::PI * f * f64::from(h) * t).sin() / f64::from(h)
                    })
                    .sum();
                out.push((v * env * 0.3) as f32);
            }
            out.extend(std::iter::repeat_n(0.0_f32, (0.06 * rate) as usize));
        }
        out.extend(std::iter::repeat_n(0.0_f32, silence));
        out
    }

    /// 計算を別のスレッドへ渡せる形であること。 入力は所有した値だけで、出力は台帳へ
    /// 書く権限を持ち出さない。 渡せなくなったら、ここが組み立てで落とす。
    #[test]
    fn 入力と出力はスレッドを渡れる() {
        fn send_sync<T: Send + Sync>() {}
        send_sync::<TakeAnalysisInput>();
        send_sync::<TakeAnalysisOutput>();
        send_sync::<ReEstimateInput>();
        send_sync::<ReEstimation>();
    }

    /// 行の事実を台帳なしで組む。 アライナは要らない（印だけを見る試験が使う）。
    fn row_for(kana: &[&str], tone: i32, owned: &[&str]) -> RowReads {
        let rules = Rules::builtin(UnitSet::Core);
        let line: Vec<Unit> = kana
            .iter()
            .map(|k| {
                koeru_core::inventory::units(UnitSet::Core)
                    .into_iter()
                    .find(|u| u.kana == *k)
                    .expect("インベントリにある仮名")
            })
            .collect();
        RowReads {
            setup: Ok(Setup {
                method: Method::Single,
                set: UnitSet::Core,
                rules,
            }),
            kana: Ok(kana.iter().map(|k| (*k).to_owned()).collect()),
            line: Ok(line),
            populations: Ok(Populations::default()),
            tone: Ok(tone),
            owned: Ok(owned.iter().map(|s| (*s).to_owned()).collect()),
        }
    }

    fn stamp(row: &RowReads) -> BasedOn {
        analysis_based_on(1, f0::Purpose::Preview, None, row)
    }

    /// 同じ入力なら同じ印。 計算が読んだものが変われば外れる。
    #[test]
    fn 読んだ入力のどれが変わっても印が外れる() {
        let base = stamp(&row_for(&["あ", "か"], 57, &["あ", "か"]));
        assert!(base.is_current_in(&stamp(&row_for(&["あ", "か"], 57, &["あ", "か"]))));
        assert!(!base.is_current_in(&stamp(&row_for(&["あ", "き"], 57, &["あ", "か"]))));
        assert!(!base.is_current_in(&stamp(&row_for(&["あ", "か"], 59, &["あ", "か"]))));
        assert!(
            !base.is_current_in(&stamp(&row_for(&["あ", "か"], 57, &["あ"]))),
            "持ち主が変わった"
        );
        let row = row_for(&["あ", "か"], 57, &["あ", "か"]);
        assert!(!base.is_current_in(&analysis_based_on(2, f0::Purpose::Preview, None, &row)));
        assert!(!base.is_current_in(&analysis_based_on(1, f0::Purpose::Distribution, None, &row)));
        assert!(!base.is_current_in(&analysis_based_on(
            1,
            f0::Purpose::Preview,
            Some(80.0),
            &row
        )));
    }

    /// 集団は印に入れない（他のテイクの統計。 変わっても置く値の正しさは崩れない）。
    /// 読めなかったことも入力の一部で、読めた印とは別になる。
    #[test]
    fn 集団は印に載せず_読めなかった事実は別の印になる() {
        let mut row = row_for(&["あ"], 57, &["あ"]);
        let base = stamp(&row);
        let mut pops = Populations::default();
        let oto = Oto {
            offset_ms: 50.0,
            consonant_ms: 60.0,
            cutoff_ms: -300.0,
            preutterance_ms: 40.0,
            overlap_ms: 20.0,
        };
        pops.push(57, "あ", &oto, 1000.0);
        row.populations = Ok(pops);
        assert!(base.is_current_in(&stamp(&row)), "集団が変わっても外れない");

        row.owned = Err(AppError::from_failure(
            koeru_core::db::LedgerError::UnknownTake,
        ));
        assert!(!base.is_current_in(&stamp(&row)));
    }

    /// 印の `Debug` と指紋は、読みや綴りの中身を出さない（`AGENTS.md` の禁止事項3）。
    #[test]
    fn 印は読みの中身を出さない() {
        let based = stamp(&row_for(&["あ", "か"], 57, &["あ", "か"]));
        let shown = format!("{based:?}");
        assert!(!shown.contains('あ'), "{shown}");
        for (_, fingerprint) in based.iter() {
            assert!(!fingerprint.contains('あ') && !fingerprint.contains('か'));
        }
    }

    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    fn input_for(kana: &[&str], samples: Vec<f32>) -> TakeAnalysisInput {
        let rules = Rules::builtin(UnitSet::Core);
        let line: Vec<Unit> = kana
            .iter()
            .map(|k| {
                koeru_core::inventory::units(UnitSet::Core)
                    .into_iter()
                    .find(|u| u.kana == *k)
                    .expect("インベントリにある仮名")
            })
            .collect();
        let owned = reclist::row_entries(&rules, Method::Single, &line)
            .into_iter()
            .map(|(alias, _)| alias)
            .collect();
        TakeAnalysisInput {
            take_id: 1,
            row_id: "r001".to_owned(),
            // `.frq` を書くのは受理する側。 計算はこの場所に触らない。
            wav_path: PathBuf::from("/nonexistent/r001_1.wav"),
            samples: Arc::new(samples),
            rate: koeru_audio::wav::MASTER_RATE_HZ,
            f0_purpose: f0::Purpose::Preview,
            f0_floor: None,
            facts: TakeFacts {
                counts: TakeGapCounts::default(),
                gaps: Vec::new(),
                guide_offset: None,
                preroll_frames: 0,
                duration_ms: 1000.0,
            },
            row: RowReads {
                setup: Ok(Setup {
                    method: Method::Single,
                    set: UnitSet::Core,
                    rules,
                }),
                kana: Ok(kana.iter().map(|k| (*k).to_owned()).collect()),
                line: Ok(line),
                populations: Ok(Populations::default()),
                tone: Ok(57),
                owned: Ok(owned),
            },
            aligner: Chosen::detect().expect("MFA のモデルを読める（submodule を取っている）"),
        }
    }

    /// `Studio` も台帳も無しで呼べて、同じ入力から同じ出力になる（`TR-ALN-29`）。
    ///
    /// 構造を見る試験。 境界の位置が正しいかは見ていない。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 同じ入力から同じ出力になる() {
        let input = input_for(&["あ", "か"], synthetic_voice(2, 180.0));
        let a = analyze_take(&input, &|| Ok(())).expect("取り消されない");
        let b = analyze_take(&input, &|| Ok(())).expect("取り消されない");
        // `f64` の `Debug` は往復できる最短の表記なので、文字列の一致はビットの一致。
        assert_eq!(format!("{a:?}"), format!("{b:?}"));

        assert!(a.failure.is_none(), "{:?}", a.failure);
        assert!(!a.f0.is_empty(), "F0 は受理する側が音域へ足す");
        assert!(matches!(a.steps.first(), Some(Step::Analysis)));
        assert!(matches!(a.steps.last(), Some(Step::Enqueue)));
        assert!(a.steps.iter().any(|s| matches!(s, Step::Measured(_))));
        // `.frq` は素材ファイル全体を hop=256 で渡す（`DEC-SYN-008`）。 切り出さない。
        assert_eq!(a.analysis.hop_size(), 256);
    }

    /// 計算が出した oto・境界・`.frq` を1つの指紋へ畳む（`DEC-PLT-053`）。
    ///
    /// 何も出ていなければ（境界も oto も空）、指紋が空の入力を畳んだだけになる。
    /// 呼び出し側が `otos` の非空を確かめる。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    fn output_fingerprint(out: &TakeAnalysisOutput) -> (usize, koeru_core::fingerprint::Digest) {
        let mut otos = Vec::new();
        let mut boundaries = Vec::new();
        for step in &out.steps {
            match step {
                Step::Entries(e) => otos.extend(e.rows.iter().map(|r| (r.alias.clone(), r.oto))),
                Step::Boundaries(b) => boundaries.extend(b.iter().cloned()),
                _ => {}
            }
        }
        let frq = out.analysis.frq.to_bytes().expect("`.frq` にできる");
        let digest = koeru_core::fingerprint::OutputFingerprint {
            otos: &otos,
            boundaries: &boundaries,
            frq: &frq,
        }
        .digest();
        (otos.len(), digest)
    }

    /// 同じ入力からの出力は同じ指紋になり、入力の音が変われば別の指紋になる
    /// （`TR-ALN-29`）。 実モデルで走る。
    ///
    /// 出力の指紋は永続化しない（`DEC-PLT-053`）。 ここで使うだけ。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 出力の指紋は入力が同じなら同じで_音が変われば変わる() {
        let run = |hz: f64| {
            let input = input_for(&["あ", "か"], synthetic_voice(2, hz));
            output_fingerprint(&analyze_take(&input, &|| Ok(())).expect("取り消されない"))
        };
        let (n, a) = run(180.0);
        let (_, b) = run(180.0);
        let (_, c) = run(240.0);
        assert!(n > 0, "oto が出ている");
        assert_eq!(a, b, "ビット単位で同じ出力は同じ指紋");
        assert_ne!(a, c, "音が変われば出力も指紋も変わる");
    }

    /// 不連続のあるテイクは確認キューへ入れない。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 取りこぼしたテイクはキューへ入れない() {
        let mut input = input_for(&["あ"], synthetic_voice(1, 180.0));
        input.facts.counts.discontinuities = 1;
        let out = analyze_take(&input, &|| Ok(())).expect("取り消されない");
        assert!(!out.steps.iter().any(|s| matches!(s, Step::Enqueue)));
    }

    /// 台帳を読めなかった段で止まり、そこまでに作れたものを返す。
    ///
    /// 規約と綴りの表を読めなければ、何も置かない。 行の読みを読めなければ、解析値は置くが
    /// 計測から先は置かない。 以前は読む場所ごとの `?` がそうしていた。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 読めなかった段で止まる() {
        let unreadable = || AppError::from_failure(koeru_core::db::LedgerError::UnknownTake);

        let mut input = input_for(&["あ"], synthetic_voice(1, 180.0));
        input.row.setup = Err(unreadable());
        let out = analyze_take(&input, &|| Ok(())).expect("取り消されない");
        assert!(out.steps.is_empty(), "何も置かない: {:?}", out.steps);
        assert!(out.failure.is_some());
        assert!(out.analysis.peak > 0.0, "画面へ返す値は作れている");

        let mut input = input_for(&["あ"], synthetic_voice(1, 180.0));
        input.row.kana = Err(unreadable());
        let out = analyze_take(&input, &|| Ok(())).expect("取り消されない");
        assert!(
            matches!(
                out.steps.as_slice(),
                [Step::Analysis, Step::AlignmentFailed(_)]
            ),
            "{:?}",
            out.steps
        );
        assert!(out.failure.is_some());
    }
}
