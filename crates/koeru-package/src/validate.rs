//! 書き出し前検証（`TR-PKG-49`, `TR-PKG-50`, `TR-PKG-17`, `TR-PKG-51`）。
//!
//! 1件でも [`Finding`] が残っていれば書き出さない（`REQ-PKG-104`）。
//! FSL はこれを「通ったか通らないか」の1つの述語に畳んでいて
//! （`packaging-export.fsl` の ASSUME-1）、個々の条件はここが持つ。
//!
//! # どこを直せばよいかまで返す
//!
//! `TR-PKG-51`。ファイルパスと行番号だけを返して終わらない。
//! [`Finding`] は「どの WAV の、どのエイリアスの、どの値が、どう不正か」を持つ。
//!
//! # フルスケール到達はここで数えない
//!
//! `TR-PKG-49` が求める関門は、録音時に台帳へ入れた計測値
//! （`TR-REC-07`、`TR-REC-16`）から出す。 ここで WAV を走査し直すと、
//! 同じ規則が2箇所に置かれる。止めない関門でもあるので、
//! 書き出しの可否を決めるこの層には要らない。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use koeru_formats::text;
use koeru_formats::wav::{WavError, WavInfo};
use koeru_model::names::{self, AliasProblem};

use crate::bank::{Sample, Subbank, VoiceBank, decorate};
use crate::profile::Profile;

/// 検証の結果（`TR-PKG-49`）。
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// これがある間は書き出さない。
    pub findings: Vec<Finding>,
    /// CP932 で書けない箇所（`TR-PKG-17`）。暗黙置換しないので、
    /// classic 互換と両対応では書き出しを止める。
    pub unencodable: Vec<Unencodable>,
}

impl Report {
    /// 書き出してよいか（`REQ-PKG-104`）。
    #[must_use]
    pub fn may_export(&self) -> bool {
        self.findings.is_empty() && self.unencodable.is_empty()
    }
}

/// 見つかった違反1件（`TR-PKG-51`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// 音源ルートからの相対パス。
    pub file: String,
    /// エイリアス。ファイル単位の指摘では `None`。
    pub alias: Option<String>,
    /// 何が不正か。
    pub problem: Problem,
}

/// 違反の種類（`TR-PKG-49`, `TR-PKG-50`）。
#[derive(Debug, Clone, PartialEq)]
pub enum Problem {
    /// `character.txt` が無い（`TR-PKG-01`）。
    CharacterTxtMissing,
    /// `character.yaml` が無い（`TR-PKG-50`）。
    CharacterYamlMissing,
    /// WAV が無い。
    WavMissing,
    /// WAV を読めない。
    WavUnreadable,
    /// WAV 長が 0 以下。
    EmptyWav,
    /// マスターが 44100 Hz でない（`TR-REC-02`）。
    WrongSampleRate { found: u32 },
    /// KOERU が扱わない形式。モノラルでない、または 16 bit / 32 bit float でない。
    UnsupportedFormat,
    /// oto の行として書けない（`=` や `,` がエイリアスに入っている）。
    MalformedOtoLine,
    /// oto の5値が範囲外（`TR-EDT-43`）。
    OtoValue {
        violation: koeru_model::oto::Violation,
    },
    /// 右ブランクの位置が先行発声より手前。
    CutoffBeforePreutterance,
    /// 右ブランクの位置がオーバーラップより手前。
    CutoffBeforeOverlap,
    /// 右ブランクの位置が子音部の終わりを越えていない。
    CutoffNotAfterConsonant,
    /// 右ブランクの位置が WAV 長を越えている。
    CutoffBeyondFile,
    /// エイリアスが音源全体で重複している（`TR-PKG-19`）。
    DuplicateAlias,
    /// エイリアスの形が `TR-PKG-18` に反する。
    AliasShape { problem: AliasProblem },
    /// WAV 名が NFC でない。
    NfdName,
    /// 小文字化すると衝突する WAV 名がある（`TR-PKG-20`）。
    CaseCollision,
    /// 同じパスの素材が2つある（`TR-PKG-26`）。
    DuplicatePath,
    /// 同じフォルダを2つの区画が使っている（`TR-PKG-01`, `TR-PKG-04`）。
    DuplicateFolder,
    /// `oto.ini` の参照と実ファイル名の大小が違う（`TR-PKG-20`）。
    CaseMismatch,
    /// `.frq` のフレーム数が WAV の長さを覆っていない（`TR-PKG-49`）。
    FrqTooShort { frames: u32, needed: u32 },
    /// `.frq` の書式が壊れている。
    FrqMalformed,
    /// `.frq` に負または非数の F0 がある。無声は 0（`TR-PKG-49`）。
    FrqUnvoicedNotZero,
    /// 周波数表が無い（`TR-PKG-05`）。同梱すると readme に書いてある。
    FrqMissing,
}

impl Problem {
    /// 送信してよい種別文字列。 パスもエイリアスも送らない。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::CharacterTxtMissing => "package.character_txt_missing",
            Self::CharacterYamlMissing => "package.character_yaml_missing",
            Self::WavMissing => "package.wav_missing",
            Self::WavUnreadable => "package.wav_unreadable",
            Self::EmptyWav => "package.empty_wav",
            Self::WrongSampleRate { .. } => "package.wrong_sample_rate",
            Self::UnsupportedFormat => "package.unsupported_format",
            Self::MalformedOtoLine => "package.malformed_oto_line",
            Self::OtoValue { .. } => "package.oto_value",
            Self::CutoffBeforePreutterance => "package.cutoff_before_preutterance",
            Self::CutoffBeforeOverlap => "package.cutoff_before_overlap",
            Self::CutoffNotAfterConsonant => "package.cutoff_not_after_consonant",
            Self::CutoffBeyondFile => "package.cutoff_beyond_file",
            Self::DuplicateAlias => "package.duplicate_alias",
            Self::AliasShape { .. } => "package.alias_shape",
            Self::NfdName => "package.nfd_name",
            Self::CaseCollision => "package.case_collision",
            Self::DuplicatePath => "package.duplicate_path",
            Self::DuplicateFolder => "package.duplicate_folder",
            Self::CaseMismatch => "package.case_mismatch",
            Self::FrqTooShort { .. } => "package.frq_too_short",
            Self::FrqMalformed => "package.frq_malformed",
            Self::FrqUnvoicedNotZero => "package.frq_unvoiced_not_zero",
            Self::FrqMissing => "package.frq_missing",
        }
    }
}

/// CP932 で書けない箇所（`TR-PKG-17`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Unencodable {
    /// どこに出ている文字か。
    pub place: Place,
    /// 書けなかった文字。重複は取り除いてある。
    pub chars: Vec<char>,
    /// 代替案。無ければ本人が決める（`TR-PKG-17`）。
    pub suggestion: Option<String>,
}

/// 書けない文字が出ている場所（`TR-PKG-51`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// `character.txt` の `name=`。
    VoiceName,
    /// `character.txt` のそのキー。
    CharacterField { key: &'static str },
    /// `oto.ini` のエイリアス。
    Alias { file: String, alias: String },
    /// `readme.txt` のその節。
    ReadmeSection { title: &'static str },
    /// 同梱する `presamp.ini`（`TR-RCL-24`）。 差し替えた表（`TR-SYN-36`）から来る。
    Presamp,
}

/// マスター WAV1本について、呼び出し側が先に調べておく事実（`TR-PKG-49`）。
///
/// 検証が要るのはヘッダの3つ（形式・レート・サンプル数）だけで、サンプルの値は見ない。
/// ファイルを開くのは呼び出し側で、ここは開かない。 全テイクを読み直すと、画面の
/// 状態を引くたびに数百 MB を読むことになる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterFact {
    /// ファイルが無い。
    Missing,
    /// 開けない、ヘッダが壊れている、`data` が宣言より短い。
    Unreadable,
    /// KOERU が扱わない形式（モノラルでない、16 bit / 32 bit float でない）。
    Unsupported,
    /// 読めた。
    Wav {
        /// ヘッダのレート。
        rate_hz: u32,
        /// サンプル数（モノラルなのでフレーム数と同じ）。
        samples: u64,
    },
}

impl MasterFact {
    /// ヘッダの読み取り結果から事実にする。
    ///
    /// ファイルが在るかどうかはここでは分からない。 無ければ [`MasterFact::Missing`] を
    /// 呼び出し側が直接置く。
    #[must_use]
    pub fn from_header(header: &Result<WavInfo, WavError>) -> Self {
        match header {
            Ok(info) => Self::Wav {
                rate_hz: info.rate_hz,
                samples: info.samples,
            },
            Err(WavError::Unsupported { .. }) => Self::Unsupported,
            Err(_) => Self::Unreadable,
        }
    }
}

/// 素材の在り処ごとの事実。
///
/// **引けなかったものは「無い」と読む。** 事実を集め損ねたときに通すと、
/// 調べていない素材を書き出してしまう。
pub type MasterFacts = BTreeMap<PathBuf, MasterFact>;

/// `bank` の素材それぞれについて `probe` で事実を集める。
///
/// 同じ在り処は1回だけ調べる。 `probe` はファイルを開く側の仕事で、ここは数え上げるだけ。
pub fn collect_facts(bank: &VoiceBank, mut probe: impl FnMut(&Path) -> MasterFact) -> MasterFacts {
    let mut facts = MasterFacts::new();
    for m in bank.subbanks.iter().flat_map(|s| &s.samples) {
        if !facts.contains_key(&m.master) {
            facts.insert(m.master.clone(), probe(&m.master));
        }
    }
    facts
}

/// 書き出し前に全件検査する（`TR-PKG-49`）。
///
/// 素材の事実（`facts`）を見る。 台帳の値だけを見ると、素材が消えている・
/// レートが違うといった、書き出したあとにしか分からない壊れ方を通す。
#[tracing::instrument(skip(bank, facts), fields(profile = profile.as_str(), subbanks = bank.subbanks.len()))]
pub fn validate(bank: &VoiceBank, profile: Profile, facts: &MasterFacts) -> Report {
    let mut report = Report::default();

    // `character.txt` と `character.yaml` は必ず生成する（`TR-PKG-01`, `TR-PKG-50`）。
    // 生成側の不具合でしか起きないが、検証項目として持つ——
    // 「作ったはずのものが入っていない」を、受け手より先に見つける。
    if bank.character.name.trim().is_empty() {
        report.findings.push(Finding {
            file: "character.txt".to_owned(),
            alias: None,
            problem: Problem::CharacterTxtMissing,
        });
    }
    if bank.subbanks.is_empty() {
        report.findings.push(Finding {
            file: "character.yaml".to_owned(),
            alias: None,
            problem: Problem::CharacterYamlMissing,
        });
    }

    check_names_and_aliases(bank, &mut report);
    for s in &bank.subbanks {
        for m in &s.samples {
            let fact = facts.get(&m.master).copied().unwrap_or(MasterFact::Missing);
            check_sample(s, m, fact, &mut report);
        }
    }
    if profile.needs_cp932() {
        report.unencodable = unencodable_places(bank);
    }
    report
}

/// 音源全体にかかる検査（`TR-PKG-19`, `TR-PKG-20`, `TR-PKG-18`）。
fn check_names_and_aliases(bank: &VoiceBank, report: &mut Report) {
    // エイリアスの全体重複。 区画ごとに見ると、フォルダをまたいだ衝突を見逃す。
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for s in &bank.subbanks {
        for m in &s.samples {
            let path = format!("{}{}", s.path_prefix(), m.file);
            for e in &m.entries {
                let alias = decorate(s, &e.alias);
                if seen.insert(alias.clone(), ()).is_some() {
                    report.findings.push(Finding {
                        file: path.clone(),
                        alias: Some(alias.clone()),
                        problem: Problem::DuplicateAlias,
                    });
                }
                for problem in names::check_alias(&alias) {
                    report.findings.push(Finding {
                        file: path.clone(),
                        alias: Some(alias.clone()),
                        problem: Problem::AliasShape { problem },
                    });
                }
                // `oto.ini` は `=` と `,` で欄を切る。含むと行として読めなくなる。
                if alias.contains('=') || alias.contains(',') || alias.contains('\n') {
                    report.findings.push(Finding {
                        file: path.clone(),
                        alias: Some(alias),
                        problem: Problem::MalformedOtoLine,
                    });
                }
            }
        }
    }

    // 区画のフォルダが重なっていないか（`TR-PKG-01`）。
    //
    // 重なると、`oto.ini` が同じ場所に2つ出る。 ZIP は同名のエントリを
    // 許すので包むところは通り、**展開した側でどちらか片方だけが残る。**
    //
    // **大小を無視して見る。** `C4` と `c4` を別のものとして扱うと、
    // Windows と既定の macOS では同じフォルダに落ちる。
    let mut folders: BTreeMap<String, ()> = BTreeMap::new();
    for s in &bank.subbanks {
        let folder = s.folder.clone().unwrap_or_default();
        if folders.insert(folder.to_lowercase(), ()).is_some() {
            report.findings.push(Finding {
                file: format!("{folder}/oto.ini"),
                alias: None,
                problem: Problem::DuplicateFolder,
            });
        }
    }

    // WAV 名の NFD と、パスの衝突。 どちらも受け手の環境でだけ壊れる。
    //
    // **生成するパス全部で見る。** WAV だけを見ると、`.frq` と `oto.ini` が
    // 漏れる。区画ごとに見ると、フォルダ名が重なった区画の間で同じパスが
    // 出ても気づけない。
    let paths = bank.generated_paths();
    let refs: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut exact: BTreeMap<&str, usize> = BTreeMap::new();
    for p in &refs {
        *exact.entry(p).or_default() += 1;
    }
    for (path, n) in exact {
        if n > 1 {
            report.findings.push(Finding {
                file: path.to_owned(),
                alias: None,
                problem: Problem::DuplicatePath,
            });
        }
    }
    for collision in names::case_collisions(&refs) {
        report.findings.push(Finding {
            file: collision,
            alias: None,
            problem: Problem::CaseCollision,
        });
    }

    for s in &bank.subbanks {
        let prefix = s.path_prefix();
        for m in &s.samples {
            if koeru_model::text::to_nfc(&m.file) != m.file {
                report.findings.push(Finding {
                    file: format!("{prefix}{}", m.file),
                    alias: None,
                    problem: Problem::NfdName,
                });
            }
            // `oto.ini` の `<wav>` 欄は実ファイル名と完全一致（`TR-PKG-20`）。
            for e in &m.entries {
                if e.file != m.file {
                    report.findings.push(Finding {
                        file: format!("{prefix}{}", m.file),
                        alias: Some(decorate(s, &e.alias)),
                        problem: Problem::CaseMismatch,
                    });
                }
            }
        }
    }
}

/// 素材1本の検査（`TR-PKG-49`）。
fn check_sample(s: &Subbank, m: &Sample, fact: MasterFact, report: &mut Report) {
    let path = format!("{}{}", s.path_prefix(), m.file);
    let mut push = |problem: Problem, alias: Option<String>| {
        report.findings.push(Finding {
            file: path.clone(),
            alias,
            problem,
        });
    };

    let (rate_hz, samples) = match fact {
        MasterFact::Missing => {
            push(Problem::WavMissing, None);
            return;
        }
        MasterFact::Unsupported => {
            push(Problem::UnsupportedFormat, None);
            return;
        }
        MasterFact::Unreadable => {
            push(Problem::WavUnreadable, None);
            return;
        }
        MasterFact::Wav { rate_hz, samples } => (rate_hz, samples),
    };
    if samples == 0 {
        push(Problem::EmptyWav, None);
        return;
    }
    // **44100 でないマスターを通さない。** 書き出しはレートを変換せずヘッダに 44100 と
    // 書くので、通すと 44100 と名乗る別のレートの音が配られる（`TR-REC-02`）。
    if rate_hz != koeru_formats::wav::DISTRIBUTION_RATE_HZ {
        push(Problem::WrongSampleRate { found: rate_hz }, None);
    }

    let frames = usize::try_from(samples).unwrap_or(usize::MAX);
    #[allow(
        clippy::cast_precision_loss,
        reason = "1テイクのサンプル数は f64 の仮数に収まる"
    )]
    let len_ms = samples as f64 * 1000.0 / f64::from(rate_hz);

    for e in &m.entries {
        let alias = Some(decorate(s, &e.alias));
        for violation in e.oto.violations(len_ms) {
            push(Problem::OtoValue { violation }, alias.clone());
        }
        let cutoff = e.oto.cutoff_position_ms(len_ms);
        if cutoff < e.oto.offset_ms + e.oto.preutterance_ms {
            push(Problem::CutoffBeforePreutterance, alias.clone());
        }
        if cutoff < e.oto.offset_ms + e.oto.overlap_ms {
            push(Problem::CutoffBeforeOverlap, alias.clone());
        }
        if cutoff <= e.oto.offset_ms + e.oto.consonant_ms {
            push(Problem::CutoffNotAfterConsonant, alias.clone());
        }
        if cutoff > len_ms {
            push(Problem::CutoffBeyondFile, alias.clone());
        }
    }

    match &m.frq {
        Some(frq) => {
            for problem in check_frq(frq, frames) {
                push(problem, None);
            }
        }
        // 同梱すると readme に書いてある（`TR-PKG-06`）。 無いまま出すと、
        // 説明書だけが嘘になる。録音時に作ると決めてあるので（`TR-PKG-05`）、
        // 無いのは作り損ねたということ。
        None => push(Problem::FrqMissing, None),
    }
}

/// `.frq` の2項目（`TR-PKG-49`）。
///
/// 書式は `koeru_formats::frq` が持つ。ここで読むのは検証に要る3つだけ
/// ——フレーム数、hop、f0 の値。
fn check_frq(bytes: &[u8], wav_frames: usize) -> Vec<Problem> {
    const HEADER: usize = 40;
    let hop = koeru_formats::frq::HOP_SIZE as usize;

    if bytes.len() < HEADER || &bytes[0..8] != b"FREQ0003" {
        return vec![Problem::FrqMalformed];
    }
    let declared_hop = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as usize;
    let frames = u32::from_le_bytes([bytes[36], bytes[37], bytes[38], bytes[39]]) as usize;
    if declared_hop != hop || bytes.len() < HEADER + frames * 16 {
        return vec![Problem::FrqMalformed];
    }

    let mut out = Vec::new();
    // フレーム数 × hop が WAV の長さを覆っているか。
    // 覆っていないと、読み手は最後のフレームの手前で止まる。
    if frames * hop < wav_frames {
        out.push(Problem::FrqTooShort {
            frames: u32::try_from(frames).unwrap_or(u32::MAX),
            needed: u32::try_from(wav_frames.div_ceil(hop)).unwrap_or(u32::MAX),
        });
    }
    // 無声は 0。 負や非数を書くと、読み手が音高として解釈する。
    let unhealthy = (0..frames).any(|i| {
        let at = HEADER + i * 16;
        let Some(chunk) = bytes.get(at..at + 8) else {
            return true;
        };
        let mut raw = [0_u8; 8];
        raw.copy_from_slice(chunk);
        let f0 = f64::from_le_bytes(raw);
        !f0.is_finite() || f0 < 0.0
    });
    if unhealthy {
        out.push(Problem::FrqUnvoicedNotZero);
    }
    out
}

/// CP932 で書けない箇所を全件挙げる（`TR-PKG-17`）。
fn unencodable_places(bank: &VoiceBank) -> Vec<Unencodable> {
    let mut out = Vec::new();
    let mut add = |place: Place, s: &str| {
        let chars = text::unencodable_chars(s);
        if !chars.is_empty() {
            out.push(Unencodable {
                place,
                chars,
                suggestion: text::cp932_fallback(s),
            });
        }
    };

    let c = &bank.character;
    add(Place::VoiceName, &c.name);
    for (key, value) in [
        ("author", c.author.as_deref()),
        ("voice", c.voice.as_deref()),
        ("sample", c.sample.as_deref()),
        ("web", c.web.as_deref()),
        ("version", c.version.as_deref()),
    ] {
        if let Some(v) = value {
            add(Place::CharacterField { key }, v);
        }
    }

    let r = &bank.readme;
    for (title, value) in [
        ("推奨音域", r.tone_range_note.as_deref()),
        ("利用規約", r.terms.as_deref()),
        ("クレジット表記例", r.credit_example.as_deref()),
        ("連絡先", r.contact.as_deref()),
        ("免責", r.disclaimer.as_deref()),
        ("キャラクター設定", r.character_note.as_deref()),
    ] {
        if let Some(v) = value {
            add(Place::ReadmeSection { title }, v);
        }
    }

    for s in &bank.subbanks {
        let prefix = s.path_prefix();
        for m in &s.samples {
            for e in &m.entries {
                let alias = decorate(s, &e.alias);
                add(
                    Place::Alias {
                        file: format!("{prefix}{}", m.file),
                        alias: alias.clone(),
                    },
                    &alias,
                );
            }
        }
    }

    // 同梱する `presamp.ini` も CP932 で書く（`tree::build`）。 書くものそのものを見る。
    //
    // **見ていなかった。** 差し替えた表（`TR-SYN-36`）に CP932 で書けない字が
    // あると——使っていない節のテンプレートでも——組み立てが落ちる。ここで
    // 拾わないと検査は通り、書き出しの的は押せるのに、押すと必ず失敗した。
    add(
        Place::Presamp,
        &koeru_model::presamp::write(&bank.rules, crate::profile::NEWLINE),
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Character, Readme, Sample, Subbank};
    use koeru_formats::oto_ini::IniEntry;
    use koeru_model::alias::Method;
    use koeru_model::oto::Oto;
    use std::cell::RefCell;
    use std::io::Cursor;

    // このクレートはファイルを開かない。 試験は、置いたことになっている WAV の事実を
    // 仮想の置き場（試験ごとのスレッドに1つ）へ書き、`validate` へはそこから集めて渡す。
    thread_local! {
        static DISK: RefCell<MasterFacts> = const { RefCell::new(MasterFacts::new()) };
    }

    fn tmp(tag: &str) -> PathBuf {
        PathBuf::from("/virtual").join(tag)
    }

    fn wav_fact(rate_hz: u32, ms: u32) -> MasterFact {
        MasterFact::Wav {
            rate_hz,
            samples: u64::from(rate_hz) * u64::from(ms) / 1000,
        }
    }

    /// 置いた事実を仮想の置き場へ書く。
    fn put(path: &Path, fact: MasterFact) {
        DISK.with(|d| d.borrow_mut().insert(path.to_path_buf(), fact));
    }

    /// 44100 Hz / 指定した長さの WAV を置いたことにする。
    fn write_wav(dir: &Path, name: &str, ms: u32) -> PathBuf {
        let path = dir.join(name);
        put(&path, wav_fact(44_100, ms));
        path
    }

    /// 仮想の置き場にあるものを事実として、検証する。 置いていないものは無い。
    fn validate(bank: &VoiceBank, profile: Profile) -> Report {
        let facts = collect_facts(bank, |p| {
            DISK.with(|d| d.borrow().get(p).copied().unwrap_or(MasterFact::Missing))
        });
        super::validate(bank, profile, &facts)
    }

    fn oto() -> Oto {
        Oto {
            offset_ms: 100.0,
            consonant_ms: 60.0,
            cutoff_ms: -300.0,
            preutterance_ms: 40.0,
            overlap_ms: 20.0,
        }
    }

    /// 1秒の WAV に見合う周波数表。 無いと違反になる（`TR-PKG-05`）。
    fn frq_for(ms: u32) -> Vec<u8> {
        let frames = (44_100_usize * ms as usize / 1000).div_ceil(256);
        frq_bytes(frames, &vec![220.0; frames])
    }

    fn sample(file: &str, master: PathBuf, aliases: &[&str]) -> Sample {
        Sample {
            file: file.to_owned(),
            master,
            frq: Some(frq_for(1000)),
            entries: aliases
                .iter()
                .map(|a| IniEntry {
                    file: file.to_owned(),
                    alias: (*a).to_owned(),
                    oto: oto(),
                })
                .collect(),
        }
    }

    fn bank(subbanks: Vec<Subbank>) -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            methods: vec![Method::Single],
            tones: vec![57],
            subbanks,
            rules: koeru_model::presamp::Rules::builtin(koeru_model::inventory::UnitSet::Core),
        }
    }

    fn subbank(folder: Option<&str>, prefix: &str, samples: Vec<Sample>) -> Subbank {
        Subbank {
            folder: folder.map(str::to_owned),
            color: folder.unwrap_or_default().to_owned(),
            voice_color: None,
            prefix: prefix.to_owned(),
            suffix: String::new(),
            tone: None,
            samples,
        }
    }

    fn kinds(r: &Report) -> Vec<&'static str> {
        r.findings.iter().map(|f| f.problem.kind()).collect()
    }

    #[test]
    fn 揃っていれば通る() {
        let d = tmp("ok");
        let w = write_wav(&d, "s001.wav", 1000);
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", w, &["あ"])],
        )]);
        let r = validate(&b, Profile::Both);
        assert!(r.may_export(), "{:?}", r.findings);
    }

    #[test]
    fn wav_が無ければ止まる() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample(
                "s001.wav",
                PathBuf::from("/nowhere/s001.wav"),
                &["あ"],
            )],
        )]);
        let r = validate(&b, Profile::Both);
        assert!(!r.may_export());
        assert_eq!(kinds(&r), ["package.wav_missing"]);
    }

    /// マスターが 44100 でないまま配ると、44100 と名乗る別のレートの音になる。
    #[test]
    fn レートが違えば止まる() {
        let d = tmp("rate");
        let path = d.join("s001.wav");
        put(&path, wav_fact(22_050, 1000));
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", path, &["あ"])],
        )]);
        let r = validate(&b, Profile::Both);
        assert_eq!(kinds(&r), ["package.wrong_sample_rate"]);
    }

    /// ヘッダの読み取りから事実を作る経路で、44100 でないマスターを止める。
    ///
    /// 事実の組み立て（`koeru-formats` のヘッダ解析）と検証の継ぎ目。 どちらか片方の
    /// 試験だけでは、レートの取り違えが間に落ちる。
    #[test]
    fn ヘッダから作った事実でもレートが違えば止まる() {
        fn master(rate_hz: u32, frames: usize) -> Vec<u8> {
            let mut out = Vec::new();
            koeru_formats::wav::write_master_header(&mut out, rate_hz, frames as u64 * 4)
                .expect("書けること");
            out.resize(out.len() + frames * 4, 0);
            out
        }
        let fact_of = |bytes: &[u8]| {
            MasterFact::from_header(&koeru_formats::wav::inspect(&mut Cursor::new(bytes)))
        };

        let d = tmp("header-rate");
        let slow = d.join("slow.wav");
        let fine = d.join("fine.wav");
        put(&slow, fact_of(&master(22_050, 22_050)));
        put(&fine, fact_of(&master(44_100, 44_100)));
        assert_eq!(fact_of(&master(22_050, 22_050)), wav_fact(22_050, 1000));

        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("a.wav", slow, &["あ"])]),
            subbank(Some("G4"), "↑", vec![sample("b.wav", fine, &["あ"])]),
        ]);
        let r = validate(&b, Profile::Both);
        assert_eq!(kinds(&r), ["package.wrong_sample_rate"]);
        assert_eq!(
            r.findings[0].problem,
            Problem::WrongSampleRate { found: 22_050 }
        );
        assert_eq!(r.findings[0].file, "C4/a.wav");
    }

    /// 長さ0・読めない・扱わない形式は、それぞれの違反になる。
    #[test]
    fn 読めない素材は種別ごとに止まる() {
        let d = tmp("unreadable");
        let cases = [
            ("empty.wav", wav_fact(44_100, 0), "package.empty_wav"),
            ("bad.wav", MasterFact::Unreadable, "package.wav_unreadable"),
            (
                "odd.wav",
                MasterFact::Unsupported,
                "package.unsupported_format",
            ),
            ("gone.wav", MasterFact::Missing, "package.wav_missing"),
        ];
        for (name, fact, kind) in cases {
            let path = d.join(name);
            put(&path, fact);
            let b = bank(vec![subbank(None, "", vec![sample(name, path, &["あ"])])]);
            assert_eq!(kinds(&validate(&b, Profile::Both)), [kind], "{name}");
        }
    }

    /// 事実を集め損ねた素材は通さない。
    #[test]
    fn 事実が引けない素材は無いものとして止まる() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample(
                "s001.wav",
                tmp("unprobed").join("s001.wav"),
                &["あ"],
            )],
        )]);
        let r = super::validate(&b, Profile::Both, &MasterFacts::new());
        assert_eq!(kinds(&r), ["package.wav_missing"]);
    }

    /// 同じ在り処は1回しか調べない。
    #[test]
    fn 事実は在り処ごとに1回だけ集める() {
        let d = tmp("probe-once");
        let shared = d.join("shared.wav");
        let b = bank(vec![
            subbank(
                Some("C4"),
                "",
                vec![sample("a.wav", shared.clone(), &["あ"])],
            ),
            subbank(Some("G4"), "↑", vec![sample("a.wav", shared, &["い"])]),
        ]);
        let mut probes = 0;
        let facts = collect_facts(&b, |_| {
            probes += 1;
            wav_fact(44_100, 1000)
        });
        assert_eq!((probes, facts.len()), (1, 1));
    }

    /// ヘッダの事実から長さを割り出す。 全部読んだときと同じ答えになる。
    #[test]
    fn 右ブランクの判定は事実のサンプル数から出る() {
        let d = tmp("cutoff-len");
        let path = d.join("s001.wav");
        // 90ms しか無い素材。 offset 100ms は WAV の外。
        put(&path, wav_fact(44_100, 90));
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", path, &["あ"])],
        )]);
        let k = kinds(&validate(&b, Profile::Both));
        assert!(k.contains(&"package.oto_value"), "{k:?}");
    }

    /// `TR-PKG-19`。区画をまたいだ重複も見つける。
    #[test]
    fn エイリアスの全体重複を見つける() {
        let d = tmp("dup");
        let a = write_wav(&d, "a.wav", 1000);
        let c = write_wav(&d, "b.wav", 1000);
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("a.wav", a, &["あ"])]),
            subbank(Some("G4"), "", vec![sample("b.wav", c, &["あ"])]),
        ]);
        let r = validate(&b, Profile::Both);
        assert_eq!(kinds(&r), ["package.duplicate_alias"]);
    }

    /// 接頭辞が付いていれば、同じ元エイリアスでも衝突しない。
    #[test]
    fn 接頭辞があれば重複しない() {
        let d = tmp("nodup");
        let a = write_wav(&d, "a.wav", 1000);
        let c = write_wav(&d, "b.wav", 1000);
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("a.wav", a, &["あ"])]),
            subbank(Some("G4"), "↑", vec![sample("b.wav", c, &["あ"])]),
        ]);
        assert!(validate(&b, Profile::Both).may_export());
    }

    #[test]
    fn エイリアスの空白と_nfd_を止める() {
        let d = tmp("alias");
        let w = write_wav(&d, "s001.wav", 1000);
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", w, &[" あ  い"])],
        )]);
        let r = validate(&b, Profile::Both);
        assert!(kinds(&r).iter().all(|k| *k == "package.alias_shape"));
        assert_eq!(r.findings.len(), 2, "前後の空白と連続の空白");
    }

    #[test]
    fn 小文字化の衝突を止める() {
        let d = tmp("case");
        let a = write_wav(&d, "A.wav", 1000);
        let c = write_wav(&d, "a2.wav", 1000);
        let b = bank(vec![subbank(
            None,
            "",
            vec![
                sample("A.wav", a, &["あ"]),
                // 実体は別ファイルだが、名前が小文字化で衝突する。
                sample("a.wav", c, &["い"]),
            ],
        )]);
        let r = validate(&b, Profile::Both);
        assert!(kinds(&r).contains(&"package.case_collision"));
    }

    #[test]
    fn oto_の参照と実ファイル名の大小不一致を止める() {
        let d = tmp("mismatch");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut m = sample("s001.wav", w, &["あ"]);
        m.entries[0].file = "S001.WAV".to_owned();
        let b = bank(vec![subbank(None, "", vec![m])]);
        assert!(kinds(&validate(&b, Profile::Both)).contains(&"package.case_mismatch"));
    }

    #[test]
    fn 右ブランクが先行発声より手前なら止まる() {
        let d = tmp("cutoff");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut m = sample("s001.wav", w, &["あ"]);
        // 使える区間を 10ms にすると、先行発声 40ms を覆えない。
        m.entries[0].oto.cutoff_ms = -10.0;
        let b = bank(vec![subbank(None, "", vec![m])]);
        let k = kinds(&validate(&b, Profile::Both));
        assert!(k.contains(&"package.cutoff_before_preutterance"), "{k:?}");
        assert!(k.contains(&"package.cutoff_not_after_consonant"));
    }

    /// `TR-PKG-05`。周波数表が無いまま出さない。readme が同梱すると書いている。
    #[test]
    fn 周波数表が無ければ止まる() {
        let d = tmp("nofrq");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut m = sample("s001.wav", w, &["あ"]);
        m.frq = None;
        let b = bank(vec![subbank(None, "", vec![m])]);
        assert_eq!(kinds(&validate(&b, Profile::Both)), ["package.frq_missing"]);
    }

    /// `TR-PKG-01`。区画のフォルダが重なると、`oto.ini` が同じ場所に2つ出る。
    #[test]
    fn 同じフォルダの区画を止める() {
        let d = tmp("dupfolder");
        let a = write_wav(&d, "a.wav", 1000);
        let c = write_wav(&d, "b.wav", 1000);
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("a.wav", a, &["あ"])]),
            subbank(Some("C4"), "↑", vec![sample("b.wav", c, &["あ"])]),
        ]);
        let k = kinds(&validate(&b, Profile::Both));
        assert!(k.contains(&"package.duplicate_folder"), "{k:?}");
    }

    /// 大小だけが違うフォルダも、受け手の環境では同じ場所に落ちる。
    #[test]
    fn 大小だけ違うフォルダを止める() {
        let d = tmp("foldcase");
        let a = write_wav(&d, "a.wav", 1000);
        let c = write_wav(&d, "b.wav", 1000);
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("a.wav", a, &["あ"])]),
            subbank(Some("c4"), "↑", vec![sample("b.wav", c, &["い"])]),
        ]);
        let k = kinds(&validate(&b, Profile::Both));
        assert!(k.contains(&"package.duplicate_folder"), "{k:?}");
    }

    /// 区画をまたいで同じパスが出たら止める（`TR-PKG-26`）。
    ///
    /// ZIP は同名のエントリを許すので、包むところでは気づけない。
    #[test]
    fn 同じパスの素材を止める() {
        let d = tmp("duppath");
        let a = write_wav(&d, "a.wav", 1000);
        let c = write_wav(&d, "b.wav", 1000);
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("s001.wav", a, &["あ"])]),
            subbank(Some("C4"), "↑", vec![sample("s001.wav", c, &["い"])]),
        ]);
        let k = kinds(&validate(&b, Profile::Both));
        assert!(k.contains(&"package.duplicate_path"), "{k:?}");
    }

    /// `TR-PKG-17`。置換せずに、どこが書けないかを全件返す。
    #[test]
    fn cp932_で書けない箇所を全件挙げる() {
        let d = tmp("cp932");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", w, &["あ"])],
        )]);
        b.character.name = "こえる🎤".to_owned();
        b.readme.contact = Some("café".to_owned());

        let r = validate(&b, Profile::Both);
        assert!(!r.may_export());
        assert_eq!(r.unencodable.len(), 2);
        assert_eq!(r.unencodable[0].place, Place::VoiceName);
        assert_eq!(r.unencodable[0].suggestion.as_deref(), Some("こえる"));
        assert_eq!(
            r.unencodable[1].place,
            Place::ReadmeSection { title: "連絡先" }
        );
        assert_eq!(r.unencodable[1].suggestion.as_deref(), Some("cafe"));

        // UTF-8 で出すなら、この検査は要らない。
        assert!(validate(&b, Profile::OpenUtau).may_export());
    }

    /// 同梱する `presamp.ini` も CP932 で書けるかを見る（`TR-PKG-17`, `TR-SYN-36`）。
    ///
    /// **見ていなかった。** 差し替えた表の、使っていない節に書けない字があるだけで
    /// 組み立てが落ちるのに、検査は通り、書き出しの的が押せてしまっていた。
    #[test]
    fn 差し替えた表の書けない字も挙げる() {
        let d = tmp("presamp-cp932");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", w, &["あ"])],
        )]);
        // 綴りには出てこない節。 それでも `presamp.ini` には書かれる。
        b.rules
            .templates
            .insert("LONG_V".to_owned(), "%V%🎤".to_owned());

        let r = validate(&b, Profile::Both);
        assert!(!r.may_export(), "書き出しの的を押させない");
        assert!(
            r.unencodable.iter().any(|u| u.place == Place::Presamp),
            "{:?}",
            r.unencodable
        );
        // 組み立ても実際に落ちる——検査がそれを先に言っている。
        assert!(crate::tree::build(&b, Profile::Both).is_err());
        assert!(
            validate(&b, Profile::OpenUtau).may_export(),
            "UTF-8 なら書ける"
        );
    }

    #[test]
    fn frq_が短ければ止まる() {
        let d = tmp("frq");
        let w = write_wav(&d, "s001.wav", 1000);
        let mut m = sample("s001.wav", w, &["あ"]);
        m.frq = Some(frq_bytes(1, &[440.0]));
        let b = bank(vec![subbank(None, "", vec![m])]);
        assert!(kinds(&validate(&b, Profile::Both)).contains(&"package.frq_too_short"));
    }

    #[test]
    fn frq_の負の_f0_を止める() {
        let d = tmp("frqneg");
        let w = write_wav(&d, "s001.wav", 10);
        let frames = (44_100_usize * 10 / 1000).div_ceil(256);
        let mut f0 = vec![440.0; frames];
        f0[0] = -1.0;
        let mut m = sample("s001.wav", w, &["あ"]);
        m.frq = Some(frq_bytes(frames, &f0));
        let b = bank(vec![subbank(None, "", vec![m])]);
        assert!(kinds(&validate(&b, Profile::Both)).contains(&"package.frq_unvoiced_not_zero"));
    }

    /// `.frq` を組み立てる。書式は `koeru_model::frq` と同じ。
    fn frq_bytes(frames: usize, f0: &[f64]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"FREQ0003");
        out.extend_from_slice(&koeru_formats::frq::HOP_SIZE.to_le_bytes());
        out.extend_from_slice(&440.0_f64.to_le_bytes());
        out.extend_from_slice(&[0_u8; 16]);
        out.extend_from_slice(&u32::try_from(frames).unwrap_or(0).to_le_bytes());
        for i in 0..frames {
            out.extend_from_slice(&f0.get(i).copied().unwrap_or(0.0).to_le_bytes());
            out.extend_from_slice(&0.1_f64.to_le_bytes());
        }
        out
    }
}
