//! 曲の取り込み（`TR-RCL-12`、`DEC-PLT-054`）。
//!
//! 2段に分ける。
//!
//! 1. [`prepare`] — ファイルの中身を読み、取り込めるかを見る。 **何も書かない。**
//!    返す [`Plan`] は、読んだ曲・題の候補・取り込めない理由（[`Diagnostic`]）を持つ
//! 2. [`commit`] — 本人が決めた題を受け取り、曲と原文と受領証を1回の短い書き手の
//!    トランザクションで入れる。 [`OperationId`] で冪等
//!
//! 構文は `koeru-formats`（バイト列と文字列だけ）が持ち、何を取り込んでよいか・何を
//! 確定するかはここが決める（`DEC-PLT-034`）。 ファイルは呼び出し側が読んで渡す。
//!
//! # 取り込めないものは失敗ではない
//!
//! 歌詞を読めない・音高が範囲外・1ノートに2音ある、は「渡されたファイルが取り込めない」
//! という予期できる結果で、本人が別のファイルを選べば済む。 [`Err`] にせず値で返す
//! （`DEC-PLT-038`）。 [`Err`] にするのは、続けられない失敗（読めないファイル、書けない台帳、
//! 失効した貸与）だけ。
//!
//! # 原文を残す
//!
//! 渡されたバイト列は、符号化も改行も直さずに project へ残す（`DEC-PLT-054`）。 曲の行は
//! 読み取った結果で、パーサを直したときに取り込み直せるのは原文があるときだけ。

use koeru_core::db::{LedgerError, OperationId, SongImport, SongImportAnswer, SongImportRequest};
use koeru_failure::{Class, Failure};
use koeru_formats::ust::{self, UstError};
use koeru_model::inventory::UnitSet;
use koeru_model::song::Song;

use crate::error::RuntimeError;
use crate::lease::ProjectLease;
use crate::runtime::ProjectRuntime;
use crate::time::now_rfc3339;

/// 取り込みが失敗した理由。 取り込めないファイルは [`Diagnostic`] で、ここには来ない。
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// ファイルを曲として読めなかった。
    #[error(transparent)]
    Parse(#[from] UstError),

    /// 貸与が失効している、実行時の内部状態が壊れている。
    #[error(transparent)]
    Runtime(#[from] RuntimeError),

    #[error(transparent)]
    Ledger(#[from] LedgerError),

    /// 先に確定した取り込みの曲の数が、同じ要求から読んだ曲の数と合わない。
    ///
    /// 同じ要求（指紋が一致）なら同じ数になる。 食い違うのは台帳が書き換えられたとき。
    #[error("先に確定した取り込みと、曲の数が合わない")]
    ReceiptMismatch,
}

impl Failure for ImportError {
    fn code(&self) -> &'static str {
        match self {
            Self::Parse(e) => e.code(),
            Self::Runtime(e) => e.code(),
            Self::Ledger(e) => e.code(),
            Self::ReceiptMismatch => "import.receipt_mismatch",
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Parse(e) => e.class(),
            Self::Runtime(e) => e.class(),
            Self::Ledger(e) => e.class(),
            Self::ReceiptMismatch => Class::Internal,
        }
    }
}

/// 取り込めない理由。 どの曲の何番目のノートかだけを持つ。
///
/// 歌詞そのものは載せない。 この値は画面にもトレースにも出る（`AGENTS.md` #3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Diagnostic {
    /// ファイルの中の何番目の曲か（0 始まり。USTX は1トラックが1曲）。
    pub song: usize,
    pub problem: Problem,
}

/// 取り込めない理由の種類。 `note` は曲の中の何番目のノートか（0 始まり）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    /// 音高が MIDI の範囲（0〜127）の外にある。
    ///
    /// 範囲外の音高は音域の計算で桁あふれする（`Song::note_out_of_midi_range`）。
    NoteOutOfRange { note: usize },
    /// 歌詞を読めないノートがある。 `TR-RCL-12` は「必要単位集合は読み込み時に算出する」
    /// と定めているので、算出できない曲は取り込まない。
    ///
    /// 台帳へ入れると、要求が空の曲が「いま歌えます」に並ぶ。 押すまで嘘だと分からない。
    UnreadableLyrics,
    /// 1ノートに1音ではない歌詞が入っている（`DEC-SYN-009`）。
    ///
    /// 全体を繋げて読めるかだけ見ていたので、`さく` のノートが通り、そこから後ろの
    /// 音高と長さが1つずつずれて鳴っていた。
    NoteNotOneMora { note: usize },
}

/// 曲を取り込めるかを見る。 取り込みと下見が同じものを通る。
///
/// 同梱プリセットはすべて Core（`preset::builtin`）で、曲を読む側（試唱、必要単位の算出）も
/// Core で読む。 ここも Core で見る。
fn diagnose(song: &Song) -> Option<Problem> {
    if let Some(note) = song.note_out_of_midi_range() {
        return Some(Problem::NoteOutOfRange { note });
    }
    if song.moras(UnitSet::Core).is_none() {
        return Some(Problem::UnreadableLyrics);
    }
    song.note_not_one_mora(UnitSet::Core)
        .map(|note| Problem::NoteNotOneMora { note })
}

/// ファイル名からディレクトリを落とす。
///
/// 原文と一緒に残すのは名前だけ。 パスには利用者名が入りうる。
fn base_name(file_name: &str) -> &str {
    file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim()
}

/// 読んだファイルの取り込み計画。 [`prepare`] が作り、[`commit`] が受け取る。
///
/// 原文のバイト列を借りたまま持つ。 [`commit`] が残すのは、ここで読んだものと
/// 同じバイト列——別のバイト列で組み直せない。
#[derive(Debug)]
pub struct Plan<'a> {
    bytes: &'a [u8],
    file_name: String,
    title_hint: String,
    songs: Vec<Song>,
    diagnostics: Vec<Diagnostic>,
}

impl Plan<'_> {
    /// 読んだ曲。 題は候補で、複数のトラックがあれば「ファイル名 — トラック名」。
    ///
    /// **確定させるのは [`commit`] に渡す題。** 候補をそのまま台帳へ入れない（`TR-RCL-12`）。
    #[must_use]
    pub fn songs(&self) -> &[Song] {
        &self.songs
    }

    /// ファイル名から採った題の候補。 **空のことがある。** そのときは本人に打たせる。
    #[must_use]
    pub fn title_hint(&self) -> &str {
        &self.title_hint
    }

    /// 取り込めない曲と、その理由。 曲ごとに最初の1つだけ。
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 取り込める（[`Self::diagnostics`] が空）か。
    #[must_use]
    pub fn is_importable(&self) -> bool {
        self.diagnostics.is_empty()
    }
}

/// ファイルの中身を読み、取り込めるかを見る。 **何も書かない。**
///
/// 形式は拡張子から決め、決められなければ中身の先頭を見る（`koeru_formats::ust::Format`）。
/// USTX は1トラックが1曲になる。
///
/// # Errors
///
/// 符号化を判定できない、ノートが1つも無い、書式が想定と違う
/// （[`ImportError::Parse`]）。
#[tracing::instrument(skip_all, fields(len = bytes.len()))]
pub fn prepare<'a>(bytes: &'a [u8], file_name: &str) -> Result<Plan<'a>, ImportError> {
    let file_name = base_name(file_name).to_owned();
    let songs = ust::parse_file(bytes, &file_name)?;
    let diagnostics = songs
        .iter()
        .enumerate()
        .filter_map(|(song, s)| diagnose(s).map(|problem| Diagnostic { song, problem }))
        .collect();
    Ok(Plan {
        bytes,
        title_hint: ust::title_hint(&file_name).to_owned(),
        file_name,
        songs,
        diagnostics,
    })
}

/// 取り込みを断った理由。 どれも失敗ではない（`DEC-PLT-038`）。 台帳は変わっていない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// 題の数が曲の数と合わない。 題は [`Plan::songs`] と同じ並びで渡す。
    TitleCount,
    /// 空の題が含まれている。 題を決めるのは本人（`TR-RCL-12`）。
    EmptyTitle,
    /// 取り込めない曲がある（[`Plan::diagnostics`] の先頭）。
    Unimportable(Diagnostic),
}

/// 取り込んだ曲。 `(曲の ID, 曲)` の並びで、ファイルの中の並び。
pub type Songs = Vec<(String, Song)>;

/// [`commit`] の結果。 どれも失敗ではない（`DEC-PLT-038`）。
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// この要求で曲を入れた。
    Imported(Songs),
    /// 同じ操作の送り直し。 曲は増えていない。 **先に返した曲の ID と同じ ID を返す。**
    Replayed(Songs),
    /// 同じ操作の識別子で、別の要求が来た。 曲は増えていない。 先に処理した取り込みの曲。
    ///
    /// 題やバイト列が違うので、この要求の曲ではない。 曲は先に確定したときの題で返る。
    OperationIdReused(Vec<String>),
    /// 受け取れない要求。 台帳は変わっていない。
    Rejected(Rejection),
}

/// 本人が決めた題で、曲と原文と受領証を入れる。
///
/// 題は [`Plan::songs`] と同じ並びで渡す。 前後の空白は落とす。 台帳へ書くのは
/// [`ProjectRuntime::writer`] を短く取る1回のトランザクションだけで、読みと検査は
/// 書き手を取る前に済ませる。
///
/// 同じ [`OperationId`] の送り直しには、書かずに先の取り込みの曲の ID を返す
/// （[`Outcome::Replayed`]）。 [`OperationId`] が違えば、原文が同じでも別の取り込みで、
/// 曲がもう1組並ぶ。
///
/// # Errors
///
/// 貸与が失効している（[`ImportError::Runtime`]）、台帳へ書けない
/// （[`ImportError::Ledger`]）。
#[tracing::instrument(skip_all, fields(count = titles.len()))]
pub fn commit(
    runtime: &ProjectRuntime,
    lease: &ProjectLease,
    operation: &OperationId,
    plan: &Plan<'_>,
    titles: &[String],
) -> Result<Outcome, ImportError> {
    if titles.len() != plan.songs.len() {
        return Ok(Outcome::Rejected(Rejection::TitleCount));
    }
    if titles.iter().any(|t| t.trim().is_empty()) {
        return Ok(Outcome::Rejected(Rejection::EmptyTitle));
    }
    if let Some(d) = plan.diagnostics.first() {
        return Ok(Outcome::Rejected(Rejection::Unimportable(*d)));
    }

    let titled: Vec<Song> = plan
        .songs
        .iter()
        .zip(titles)
        .map(|(song, title)| Song {
            title: title.trim().to_owned(),
            ..song.clone()
        })
        .collect();
    // 返す ID は新しく振る。 送り直しのときは、台帳が持っている先の ID に置き換わる。
    let with_ids: Songs = titled
        .into_iter()
        .map(|song| (uuid::Uuid::new_v4().to_string(), song))
        .collect();

    let imported_at = now_rfc3339();
    let request = SongImportRequest {
        operation,
        file_name: &plan.file_name,
        bytes: plan.bytes,
        songs: &with_ids,
        imported_at: &imported_at,
    };
    // 書き手は文の終わりで手放す（`ProjectRuntime::writer`）。
    let result = runtime.writer(lease)?.commit_song_import(&request)?;

    Ok(match result {
        SongImport::Imported(_) => {
            tracing::info!(count = with_ids.len(), "曲を取り込んだ");
            Outcome::Imported(with_ids)
        }
        SongImport::Answered(SongImportAnswer::Replayed(receipt)) => {
            if receipt.song_ids.len() != with_ids.len() {
                return Err(ImportError::ReceiptMismatch);
            }
            tracing::info!(
                count = with_ids.len(),
                "同じ操作の取り込みの送り直しに答えた"
            );
            Outcome::Replayed(
                receipt
                    .song_ids
                    .into_iter()
                    .zip(with_ids)
                    .map(|(id, (_, song))| (id, song))
                    .collect(),
            )
        }
        SongImport::Answered(SongImportAnswer::OperationIdReused(receipt)) => {
            tracing::info!("同じ操作の識別子で別の取り込みが来た");
            Outcome::OperationIdReused(receipt.song_ids)
        }
    })
}
