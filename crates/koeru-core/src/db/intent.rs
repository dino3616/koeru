//! 録る前の予定と、テイクを確定した受領証（`DEC-REC-010`、`project-storage.fsl` の
//! ASSUME-8・9）。
//!
//! 台帳の側の手は FSL の手と次のように対応する。 ファイルの側（書きかけを開く、確定する、
//! 起動時に見て回る）は `crate::capture` と `koeru-audio` が持つ。
//!
//! | FSL | ここ |
//! |---|---|
//! | `declare_capture` | [`Ledger::declare_capture`] |
//! | `commit_take` | [`Ledger::commit_capture`] |
//! | `adopt_orphan` | [`Ledger::commit_capture`]（孤児の予定から） |
//! | `discard_invalid_take` / `dismiss_orphan` | [`Ledger::discard_capture`] |
//! | `retry_commit` | [`Ledger::answer_retry`] と、[`Ledger::commit_capture`] の冒頭 |
//! | 落ちたあとの `active = none`、`abandon_stale_intent` | [`Ledger::mark_leftover`] |
//!
//! 落ちたこと自体は台帳に書けない。 落ちたときに録音に使っていた予定は `open` のまま残り、
//! 次に開いたときの検証がファイルを見て [`Leftover`] の印を付ける。

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use uuid::Uuid;

use super::{FinalizedTake, Ledger, LedgerError, Result, db, insert_take_row};
use crate::schema::{capture_intents, commit_receipts};

/// 閉じていない予定の状態の表記。 FSL の `open` に数えるもの。
pub(crate) const OPEN_STATES: [&str; 3] = ["open", "orphaned", "partial"];

macro_rules! uuid_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        ///
        /// 表記は小文字・ハイフン区切りの UUID に揃える。 同じ値を別の書き方で送られても
        /// 同じ識別子として扱う。
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// 新しく振る。
            #[must_use]
            pub fn generate() -> Self {
                Self(Uuid::new_v4().hyphenated().to_string())
            }

            /// 外から来た表記を読む。 UUID でなければ `None`。
            #[must_use]
            pub fn parse(s: &str) -> Option<Self> {
                Uuid::try_parse(s)
                    .ok()
                    .map(|u| Self(u.hyphenated().to_string()))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// 台帳に書いてあった表記。 書くときに揃えてあるので、読み直さない。
            fn stored(s: String) -> Self {
                Self(s)
            }
        }
    };
}

uuid_id!(
    /// 録音1回の識別子（`specs/application/schema/shared.graphql` の `CaptureId`）。
    ///
    /// テイクとは別。 録り始めてから確定するか捨てるまでの1回を指す。
    CaptureId
);

uuid_id!(
    /// 確定を伴う意図の識別子（`DEC-PLT-035` の `operationId`）。
    ///
    /// 振るのは呼び出し側。 同じ値の送り直しには、受領証から同じ結果を返す。
    OperationId
);

/// 予定の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentState {
    /// 録音に使っている。 同時に1つまで。
    Open,
    /// 確定した WAV を持ったまま、テイクにならずに残った。 本人が採るか捨てるまで残る。
    Orphaned,
    /// 書きかけ（`.wav.part`）だけを持って残った。
    Partial,
    /// テイクの行と一緒に閉じた。
    Committed,
    /// テイクにならずに閉じた。
    Discarded,
    /// ファイルを持たずに残った予定に付けた印。 消さない。
    Abandoned,
}

impl IntentState {
    /// 台帳での表記。送信してよい固定語彙。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Orphaned => "orphaned",
            Self::Partial => "partial",
            Self::Committed => "committed",
            Self::Discarded => "discarded",
            Self::Abandoned => "abandoned",
        }
    }

    fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "open" => Self::Open,
            "orphaned" => Self::Orphaned,
            "partial" => Self::Partial,
            "committed" => Self::Committed,
            "discarded" => Self::Discarded,
            "abandoned" => Self::Abandoned,
            _ => return Err(LedgerError::UnknownIntentState),
        })
    }

    /// 閉じていないか。 FSL の `open` に数えるもの。
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Open | Self::Orphaned | Self::Partial)
    }
}

/// 落ちたあとに残った予定へ付ける印。 付けられるのは録音に使っていた予定だけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leftover {
    Orphaned,
    Partial,
    Abandoned,
}

impl Leftover {
    const fn state(self) -> IntentState {
        match self {
            Self::Orphaned => IntentState::Orphaned,
            Self::Partial => IntentState::Partial,
            Self::Abandoned => IntentState::Abandoned,
        }
    }
}

/// 予定1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureIntent {
    pub capture: CaptureId,
    pub row_id: String,
    /// 録音の条件は収録セッションが持つ（`TR-REC-30`）。
    pub session_id: i32,
    /// 確定したときの WAV の場所。 プロジェクトの根からの相対パス。
    pub rel_path: String,
    pub declared_at: String,
    pub state: IntentState,
    pub closed_at: Option<String>,
    pub take_id: Option<i32>,
}

/// 録る前に書く予定。
#[derive(Debug, Clone, Copy)]
pub struct NewIntent<'a> {
    pub capture: &'a CaptureId,
    pub row_id: &'a str,
    pub session_id: i32,
    /// 確定したときの WAV の場所。 プロジェクトの根からの相対パス。
    pub rel_path: &'a str,
    pub declared_at: &'a str,
}

/// テイクを確定する要求。
///
/// 行・収録セッション・WAV の場所は受け取らない。 予定が持っているものを使う——
/// 別々に渡せると、予定と違う行へテイクを載せられる。
#[derive(Debug, Clone, Copy)]
pub struct CommitRequest<'a> {
    pub operation: &'a OperationId,
    pub capture: &'a CaptureId,
    pub frames: i64,
    pub recorded_at: &'a str,
    /// 取りこぼしが無かったか（`TR-REC-07`）。 無効でもテイクの行は残す。
    pub valid: bool,
}

/// テイクを確定した受領証。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Receipt {
    pub operation: OperationId,
    pub capture: CaptureId,
    pub take_id: i32,
    pub committed_at: String,
}

/// 確定せずに返した答え。 どれも失敗ではない（`DEC-PLT-038`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// 同じ操作の送り直し。 先に確定したときの受領証。
    Replayed(Receipt),
    /// 同じ操作の識別子で、別の録音を確定しようとした。 先に処理した操作の受領証。
    OperationIdReused(Receipt),
    /// 録音はもう閉じている。 テイクになって閉じたなら、その受領証。
    CaptureClosed(Option<Receipt>),
}

/// [`Ledger::commit_capture`] の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Commit {
    /// この要求でテイクを確定した。
    Committed(Receipt),
    /// 確定せずに答えた。 テイクは増えていない。
    Answered(Answer),
}

/// 予定を読む前に決まる答え。
enum Precheck {
    Answer(Answer),
    Open(CaptureIntent),
}

/// トランザクションの中で起きた失敗。 diesel の失敗と台帳の判断を分けて持つ。
enum Tx {
    Db(diesel::result::Error),
    Ledger(LedgerError),
}

impl From<diesel::result::Error> for Tx {
    fn from(e: diesel::result::Error) -> Self {
        Self::Db(e)
    }
}

fn tx(op: &'static str) -> impl FnOnce(Tx) -> LedgerError {
    move |e| match e {
        Tx::Db(source) => LedgerError::Db { op, source },
        Tx::Ledger(e) => e,
    }
}

type Row = (
    String,
    String,
    i32,
    String,
    String,
    String,
    Option<String>,
    Option<i32>,
);

const INTENT_COLUMNS: (
    capture_intents::capture_id,
    capture_intents::row_id,
    capture_intents::session_id,
    capture_intents::rel_path,
    capture_intents::declared_at,
    capture_intents::state,
    capture_intents::closed_at,
    capture_intents::take_id,
) = (
    capture_intents::capture_id,
    capture_intents::row_id,
    capture_intents::session_id,
    capture_intents::rel_path,
    capture_intents::declared_at,
    capture_intents::state,
    capture_intents::closed_at,
    capture_intents::take_id,
);

fn build(row: Row) -> Result<CaptureIntent> {
    let (capture, row_id, session_id, rel_path, declared_at, state, closed_at, take_id) = row;
    Ok(CaptureIntent {
        capture: CaptureId::stored(capture),
        row_id,
        session_id,
        rel_path,
        declared_at,
        state: IntentState::parse(&state)?,
        closed_at,
        take_id,
    })
}

fn load_intent(
    c: &mut SqliteConnection,
    capture: &CaptureId,
) -> std::result::Result<Option<CaptureIntent>, Tx> {
    let row = capture_intents::table
        .filter(capture_intents::capture_id.eq(capture.as_str()))
        .select(INTENT_COLUMNS)
        .first::<Row>(c)
        .optional()?;
    row.map(build).transpose().map_err(Tx::Ledger)
}

type ReceiptRow = (String, String, i32, String);

fn build_receipt((operation, capture, take_id, committed_at): ReceiptRow) -> Receipt {
    Receipt {
        operation: OperationId::stored(operation),
        capture: CaptureId::stored(capture),
        take_id,
        committed_at,
    }
}

const RECEIPT_COLUMNS: (
    commit_receipts::operation_id,
    commit_receipts::capture_id,
    commit_receipts::take_id,
    commit_receipts::committed_at,
) = (
    commit_receipts::operation_id,
    commit_receipts::capture_id,
    commit_receipts::take_id,
    commit_receipts::committed_at,
);

fn receipt_by_operation(
    c: &mut SqliteConnection,
    op: &OperationId,
) -> QueryResult<Option<Receipt>> {
    commit_receipts::table
        .find(op.as_str())
        .select(RECEIPT_COLUMNS)
        .first::<ReceiptRow>(c)
        .optional()
        .map(|r| r.map(build_receipt))
}

fn receipt_by_capture(
    c: &mut SqliteConnection,
    capture: &CaptureId,
) -> QueryResult<Option<Receipt>> {
    commit_receipts::table
        .filter(commit_receipts::capture_id.eq(capture.as_str()))
        .select(RECEIPT_COLUMNS)
        .first::<ReceiptRow>(c)
        .optional()
        .map(|r| r.map(build_receipt))
}

/// 受領証と予定の状態だけで答えられるかを見る。
///
/// 受領証を先に見る。 応答が失われた consumer の送り直しは、予定がもう閉じているので、
/// 予定から見ると「閉じた録音への確定」になる——それを衝突ではなく送り直しとして答える。
fn precheck(
    c: &mut SqliteConnection,
    op: &OperationId,
    capture: &CaptureId,
) -> std::result::Result<Precheck, Tx> {
    if let Some(r) = receipt_by_operation(c, op)? {
        return Ok(Precheck::Answer(if r.capture == *capture {
            Answer::Replayed(r)
        } else {
            Answer::OperationIdReused(r)
        }));
    }
    let intent = load_intent(c, capture)?.ok_or(Tx::Ledger(LedgerError::UnknownCapture))?;
    Ok(match intent.state {
        IntentState::Committed => {
            Precheck::Answer(Answer::CaptureClosed(receipt_by_capture(c, capture)?))
        }
        IntentState::Discarded | IntentState::Abandoned => {
            Precheck::Answer(Answer::CaptureClosed(None))
        }
        IntentState::Open | IntentState::Orphaned | IntentState::Partial => Precheck::Open(intent),
    })
}

fn insert_receipt(
    c: &mut SqliteConnection,
    op: &OperationId,
    capture: &CaptureId,
    take_id: i32,
    at: &str,
) -> QueryResult<Receipt> {
    diesel::insert_into(commit_receipts::table)
        .values((
            commit_receipts::operation_id.eq(op.as_str()),
            commit_receipts::capture_id.eq(capture.as_str()),
            commit_receipts::take_id.eq(take_id),
            commit_receipts::committed_at.eq(at),
        ))
        .execute(c)?;
    Ok(Receipt {
        operation: op.clone(),
        capture: capture.clone(),
        take_id,
        committed_at: at.to_owned(),
    })
}

/// 予定を経ずに載せたテイクのために、閉じた予定と受領証を作る（[`Ledger::commit_take`]）。
/// 呼び出し側のトランザクションの中で呼ぶ。
pub(super) fn close_undeclared(
    c: &mut SqliteConnection,
    t: &FinalizedTake,
    take_id: i32,
) -> QueryResult<()> {
    let capture = CaptureId::generate();
    let rel_path = t.rel_path.replace('\\', "/");
    diesel::insert_into(capture_intents::table)
        .values((
            capture_intents::capture_id.eq(capture.as_str()),
            capture_intents::row_id.eq(&t.row_id),
            capture_intents::session_id.eq(t.session_id),
            capture_intents::rel_path.eq(&rel_path),
            capture_intents::declared_at.eq(&t.recorded_at),
            capture_intents::state.eq(IntentState::Committed.as_str()),
            capture_intents::closed_at.eq(&t.recorded_at),
            capture_intents::take_id.eq(take_id),
        ))
        .execute(c)?;
    insert_receipt(
        c,
        &OperationId::generate(),
        &capture,
        take_id,
        &t.recorded_at,
    )?;
    Ok(())
}

impl Ledger {
    /// 録る前に予定を書いてコミットする（FSL の `declare_capture`）。
    ///
    /// 録音に使っている予定は同時に1つまで。 落ちたときの予定が `open` のまま残っていれば、
    /// 先に検証（`crate::capture::verify`）で印を付ける。
    ///
    /// # Errors
    ///
    /// 行が無い（[`LedgerError::UnknownRow`]）、録音に使っている予定がもうある
    /// （[`LedgerError::CaptureAlreadyOpen`]）。
    #[tracing::instrument(skip(self, n), fields(row = %n.row_id))]
    pub fn declare_capture(&mut self, n: &NewIntent<'_>) -> Result<()> {
        self.require_row(n.row_id)?;
        let normalized_path;
        let rel_path = if n.rel_path.contains('\\') {
            normalized_path = n.rel_path.replace('\\', "/");
            &normalized_path
        } else {
            n.rel_path
        };
        self.conn
            .transaction::<_, Tx, _>(|c| {
                let busy: i64 = capture_intents::table
                    .filter(capture_intents::state.eq(IntentState::Open.as_str()))
                    .count()
                    .get_result(c)?;
                if busy > 0 {
                    return Err(Tx::Ledger(LedgerError::CaptureAlreadyOpen));
                }
                diesel::insert_into(capture_intents::table)
                    .values((
                        capture_intents::capture_id.eq(n.capture.as_str()),
                        capture_intents::row_id.eq(n.row_id),
                        capture_intents::session_id.eq(n.session_id),
                        capture_intents::rel_path.eq(rel_path),
                        capture_intents::declared_at.eq(n.declared_at),
                        capture_intents::state.eq(IntentState::Open.as_str()),
                    ))
                    .execute(c)?;
                Ok(())
            })
            .map_err(tx("declare_capture"))
    }

    /// 予定を閉じてテイクの行を足し、受領証を残す。 3つを同じトランザクションで書く
    /// （FSL の `commit_take`、孤児からなら `adopt_orphan`）。
    ///
    /// 呼べるのは WAV の fsync・rename・ディレクトリの fsync が済んだあとだけ
    /// （`DEC-REC-004`）。
    ///
    /// 受領証が既にあれば何も書かずに答える。 同じ操作の送り直しは
    /// [`Answer::Replayed`]、同じ識別子で別の録音なら [`Answer::OperationIdReused`]。
    /// 閉じた予定は再び閉じない（[`Answer::CaptureClosed`]）ので、テイクは増えない。
    ///
    /// # Errors
    ///
    /// 予定が無い（[`LedgerError::UnknownCapture`]）、書きかけしか持たない
    /// （[`LedgerError::CaptureNotFinalized`]）。
    #[tracing::instrument(skip(self, r), fields(frames = r.frames))]
    pub fn commit_capture(&mut self, r: &CommitRequest<'_>) -> Result<Commit> {
        self.conn
            .transaction::<_, Tx, _>(|c| {
                let intent = match precheck(c, r.operation, r.capture)? {
                    Precheck::Answer(a) => return Ok(Commit::Answered(a)),
                    Precheck::Open(i) => i,
                };
                if intent.state == IntentState::Partial {
                    return Err(Tx::Ledger(LedgerError::CaptureNotFinalized));
                }
                let take_id = insert_take_row(
                    c,
                    &FinalizedTake {
                        row_id: intent.row_id,
                        session_id: intent.session_id,
                        rel_path: intent.rel_path,
                        frames: r.frames,
                        recorded_at: r.recorded_at.to_owned(),
                    },
                    r.valid,
                )?;
                diesel::update(
                    capture_intents::table
                        .filter(capture_intents::capture_id.eq(r.capture.as_str())),
                )
                .set((
                    capture_intents::state.eq(IntentState::Committed.as_str()),
                    capture_intents::closed_at.eq(r.recorded_at),
                    capture_intents::take_id.eq(take_id),
                ))
                .execute(c)?;
                let receipt = insert_receipt(c, r.operation, r.capture, take_id, r.recorded_at)?;
                Ok(Commit::Committed(receipt))
            })
            .map_err(tx("commit_capture"))
    }

    /// 確定せずに答えられるなら答える。 閉じていない予定なら `None`。
    ///
    /// 録音を止める前に呼ぶ。 送り直しが来たときには録音はもう止まっているので、
    /// 「録音していない」と断る前にここで受領証を返す。
    ///
    /// # Errors
    ///
    /// 受領証も予定も無い（[`LedgerError::UnknownCapture`]）。
    pub fn answer_retry(
        &mut self,
        operation: &OperationId,
        capture: &CaptureId,
    ) -> Result<Option<Answer>> {
        self.conn
            .transaction::<_, Tx, _>(|c| {
                Ok(match precheck(c, operation, capture)? {
                    Precheck::Answer(a) => Some(a),
                    Precheck::Open(_) => None,
                })
            })
            .map_err(tx("answer_retry"))
    }

    /// テイクにせずに予定を閉じる（FSL の `discard_invalid_take`、孤児なら `dismiss_orphan`）。
    ///
    /// ファイルには触らない。 孤児を捨てるなら、WAV をどうするかは呼び出し側が決める。
    ///
    /// # Errors
    ///
    /// 予定が無い（[`LedgerError::UnknownCapture`]）、もう閉じている
    /// （[`LedgerError::CaptureNotOpen`]）。
    pub fn discard_capture(&mut self, capture: &CaptureId, at: &str) -> Result<()> {
        self.close_open(capture, &OPEN_STATES, IntentState::Discarded, Some(at))
    }

    /// 落ちたときに録音に使っていた予定へ印を付ける（`crate::capture`）。
    ///
    /// 付けられるのは `open` の予定だけ。 一度付けた印は、本人の操作（採る・捨てる）でしか
    /// 動かさない——検証を繰り返すたびに孤児が別のものへ読み替わらない。
    ///
    /// # Errors
    ///
    /// 予定が無い（[`LedgerError::UnknownCapture`]）、録音に使っていない
    /// （[`LedgerError::CaptureNotOpen`]）。
    pub fn mark_leftover(&mut self, capture: &CaptureId, to: Leftover, at: &str) -> Result<()> {
        let closed_at = (to == Leftover::Abandoned).then_some(at);
        self.close_open(capture, &["open"], to.state(), closed_at)
    }

    fn close_open(
        &mut self,
        capture: &CaptureId,
        from: &[&str],
        to: IntentState,
        closed_at: Option<&str>,
    ) -> Result<()> {
        self.conn
            .transaction::<_, Tx, _>(|c| {
                let n = diesel::update(
                    capture_intents::table
                        .filter(capture_intents::capture_id.eq(capture.as_str()))
                        .filter(capture_intents::state.eq_any(from)),
                )
                .set((
                    capture_intents::state.eq(to.as_str()),
                    capture_intents::closed_at.eq(closed_at),
                ))
                .execute(c)?;
                if n == 0 {
                    return Err(Tx::Ledger(if load_intent(c, capture)?.is_some() {
                        LedgerError::CaptureNotOpen
                    } else {
                        LedgerError::UnknownCapture
                    }));
                }
                Ok(())
            })
            .map_err(tx("close_capture"))
    }

    /// 予定1行。 無ければ `None`。
    ///
    /// # Errors
    ///
    /// 状態の表記が読めない（[`LedgerError::UnknownIntentState`]）。
    pub fn intent(&mut self, capture: &CaptureId) -> Result<Option<CaptureIntent>> {
        load_intent(&mut self.conn, capture).map_err(tx("intent"))
    }

    /// 予定を書いた順に全部。
    ///
    /// # Errors
    ///
    /// 状態の表記が読めない（[`LedgerError::UnknownIntentState`]）。
    pub fn intents(&mut self) -> Result<Vec<CaptureIntent>> {
        self.load_intents(None)
    }

    /// 閉じていない予定を書いた順に。
    ///
    /// # Errors
    ///
    /// 状態の表記が読めない（[`LedgerError::UnknownIntentState`]）。
    pub fn open_intents(&mut self) -> Result<Vec<CaptureIntent>> {
        self.load_intents(Some(&OPEN_STATES))
    }

    fn load_intents(&mut self, states: Option<&[&str]>) -> Result<Vec<CaptureIntent>> {
        let mut q = capture_intents::table
            .order(capture_intents::id.asc())
            .select(INTENT_COLUMNS)
            .into_boxed();
        if let Some(s) = states {
            q = q.filter(capture_intents::state.eq_any(s.to_vec()));
        }
        q.load::<Row>(&mut self.conn)
            .map_err(db("intents"))?
            .into_iter()
            .map(build)
            .collect()
    }

    /// その場所を、どれかの予定かテイクが既に持っているか。
    ///
    /// 新しい録音の場所を決めるときに見る（`crate::capture::free_take_path`）。 持たれた場所へ
    /// 書くと、rename が孤児の WAV を上書きする。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn path_is_taken(&mut self, rel_path: &str) -> Result<bool> {
        use crate::schema::takes;
        let normalized = rel_path.replace('\\', "/");
        let by_intent: i64 = capture_intents::table
            .filter(capture_intents::rel_path.eq(&normalized))
            .count()
            .get_result(&mut self.conn)
            .map_err(db("path_is_taken"))?;
        let by_take: i64 = takes::table
            .filter(takes::rel_path.eq(&normalized))
            .count()
            .get_result(&mut self.conn)
            .map_err(db("path_is_taken"))?;
        Ok(by_intent + by_take > 0)
    }

    /// 操作の受領証。 無ければ `None`。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn receipt(&mut self, operation: &OperationId) -> Result<Option<Receipt>> {
        receipt_by_operation(&mut self.conn, operation).map_err(db("receipt"))
    }

    /// 受領証を確定した順に全部。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn receipts(&mut self) -> Result<Vec<Receipt>> {
        commit_receipts::table
            .order(commit_receipts::take_id.asc())
            .select(RECEIPT_COLUMNS)
            .load::<ReceiptRow>(&mut self.conn)
            .map(|v| v.into_iter().map(build_receipt).collect())
            .map_err(db("receipts"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SessionSnapshot;
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn ledger() -> (Ledger, Vec<String>, i32) {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 3).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        let sid = l
            .start_session(&SessionSnapshot {
                started_at: "2026-09-26T00:00:00Z".into(),
                device_id: "test".into(),
                sample_rate_hz: 48_000,
                channels: 1,
                effects_state: "clean".into(),
                route: "test".into(),
                source_channel: 0,
                master_rate_hz: 44_100,
                resampler: "test".into(),
                upstream_conversion: "unknown".into(),
            })
            .expect("始められる");
        (l, list.into_iter().map(|r| r.id).collect(), sid)
    }

    fn declare(l: &mut Ledger, row: &str, sid: i32, path: &str) -> CaptureId {
        let capture = CaptureId::generate();
        l.declare_capture(&NewIntent {
            capture: &capture,
            row_id: row,
            session_id: sid,
            rel_path: path,
            declared_at: "2026-09-26T00:00:01Z",
        })
        .expect("予定を書ける");
        capture
    }

    fn commit(l: &mut Ledger, op: &OperationId, capture: &CaptureId) -> Commit {
        l.commit_capture(&CommitRequest {
            operation: op,
            capture,
            frames: 44_100,
            recorded_at: "2026-09-26T00:00:02Z",
            valid: true,
        })
        .expect("確定できる")
    }

    #[test]
    fn 表記を揃えて読む() {
        let a = OperationId::parse("67E55044-10B1-426F-9247-BB680E5FE0C8").expect("読める");
        let b = OperationId::parse("67e5504410b1426f9247bb680e5fe0c8").expect("読める");
        assert_eq!(a, b);
        assert_eq!(a.as_str(), "67e55044-10b1-426f-9247-bb680e5fe0c8");
        assert!(OperationId::parse("take-1").is_none());
        assert!(CaptureId::parse("").is_none());
    }

    #[test]
    fn 録音に使う予定は同時に1つまで() {
        let (mut l, rows, sid) = ledger();
        let first = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        let second = CaptureId::generate();
        let e = l
            .declare_capture(&NewIntent {
                capture: &second,
                row_id: &rows[1],
                session_id: sid,
                rel_path: "audio/b_1.wav",
                declared_at: "2026-09-26T00:00:01Z",
            })
            .expect_err("2つ目は書けない");
        assert!(matches!(e, LedgerError::CaptureAlreadyOpen), "{e:?}");

        // 閉じれば次を書ける。
        l.discard_capture(&first, "2026-09-26T00:00:02Z")
            .expect("閉じられる");
        declare(&mut l, &rows[1], sid, "audio/b_1.wav");
    }

    #[test]
    fn 知らない行には予定を書かない() {
        let (mut l, _, sid) = ledger();
        let e = l
            .declare_capture(&NewIntent {
                capture: &CaptureId::generate(),
                row_id: "無い行",
                session_id: sid,
                rel_path: "audio/x_1.wav",
                declared_at: "2026-09-26T00:00:01Z",
            })
            .expect_err("書けない");
        assert!(matches!(e, LedgerError::UnknownRow), "{e:?}");
        assert!(l.intents().expect("読める").is_empty());
    }

    #[test]
    fn 確定はテイクと予定と受領証を一度に書く() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        let op = OperationId::generate();
        let Commit::Committed(receipt) = commit(&mut l, &op, &capture) else {
            panic!("確定すること");
        };

        let takes = l.takes_of(&rows[0]).expect("読める");
        assert_eq!(takes.len(), 1);
        assert_eq!(takes[0].id, receipt.take_id);
        assert_eq!(takes[0].rel_path, "audio/a_1.wav", "予定の場所に載る");
        let intent = l.intent(&capture).expect("読める").expect("ある");
        assert_eq!(intent.state, IntentState::Committed);
        assert_eq!(intent.take_id, Some(receipt.take_id));
        assert_eq!(l.receipt(&op).expect("読める"), Some(receipt));
    }

    #[test]
    fn 送り直しには受領証で答えてテイクを増やさない() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        let op = OperationId::generate();
        let Commit::Committed(first) = commit(&mut l, &op, &capture) else {
            panic!("確定すること");
        };

        assert_eq!(
            commit(&mut l, &op, &capture),
            Commit::Answered(Answer::Replayed(first.clone()))
        );
        assert_eq!(
            l.answer_retry(&op, &capture).expect("答えられる"),
            Some(Answer::Replayed(first.clone()))
        );
        // 別の識別子で同じ録音を確定し直しても、閉じた予定は再び閉じない。
        assert_eq!(
            commit(&mut l, &OperationId::generate(), &capture),
            Commit::Answered(Answer::CaptureClosed(Some(first)))
        );
        assert_eq!(l.takes_of(&rows[0]).expect("読める").len(), 1);
        assert_eq!(l.receipts().expect("読める").len(), 1);
    }

    #[test]
    fn 同じ識別子で別の録音を確定しようとしたら衝突として答える() {
        let (mut l, rows, sid) = ledger();
        let a = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        let op = OperationId::generate();
        let Commit::Committed(first) = commit(&mut l, &op, &a) else {
            panic!("確定すること");
        };
        let b = declare(&mut l, &rows[1], sid, "audio/b_1.wav");
        assert_eq!(
            commit(&mut l, &op, &b),
            Commit::Answered(Answer::OperationIdReused(first))
        );
        assert!(l.takes_of(&rows[1]).expect("読める").is_empty());
        let intent = l.intent(&b).expect("読める").expect("ある");
        assert_eq!(intent.state, IntentState::Open, "使い回しでは閉じない");
    }

    #[test]
    fn 無効なテイクも予定を閉じて受領証を残す() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        let op = OperationId::generate();
        let got = l
            .commit_capture(&CommitRequest {
                operation: &op,
                capture: &capture,
                frames: 100,
                recorded_at: "2026-09-26T00:00:02Z",
                valid: false,
            })
            .expect("確定できる");
        assert!(matches!(got, Commit::Committed(_)), "{got:?}");
        let takes = l.takes_of(&rows[0]).expect("読める");
        assert!(takes[0].invalid);
        assert_eq!(
            l.row_state(&rows[0]).expect("読める"),
            crate::db::RowState::Unrecorded,
            "無効なテイクは採用を動かさない"
        );
    }

    #[test]
    fn 書きかけしか持たない予定はテイクにしない() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        l.mark_leftover(&capture, Leftover::Partial, "2026-09-26T00:00:03Z")
            .expect("印を付けられる");
        let e = l
            .commit_capture(&CommitRequest {
                operation: &OperationId::generate(),
                capture: &capture,
                frames: 1,
                recorded_at: "2026-09-26T00:00:04Z",
                valid: true,
            })
            .expect_err("確定できない");
        assert!(matches!(e, LedgerError::CaptureNotFinalized), "{e:?}");
        assert!(l.takes_of(&rows[0]).expect("読める").is_empty());
    }

    #[test]
    fn 孤児の予定から採れる() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        l.mark_leftover(&capture, Leftover::Orphaned, "2026-09-26T00:00:03Z")
            .expect("印を付けられる");
        // 印を付けたら、次の録音の予定を書ける（`active = none`）。
        declare(&mut l, &rows[1], sid, "audio/b_1.wav");

        let got = commit(&mut l, &OperationId::generate(), &capture);
        assert!(matches!(got, Commit::Committed(_)), "{got:?}");
        assert_eq!(l.takes_of(&rows[0]).expect("読める").len(), 1);
    }

    #[test]
    fn 付けた印は検証で動かさない() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        l.mark_leftover(&capture, Leftover::Orphaned, "2026-09-26T00:00:03Z")
            .expect("印を付けられる");
        let e = l
            .mark_leftover(&capture, Leftover::Abandoned, "2026-09-26T00:00:04Z")
            .expect_err("孤児を放棄に読み替えない");
        assert!(matches!(e, LedgerError::CaptureNotOpen), "{e:?}");
        let e = l
            .mark_leftover(&CaptureId::generate(), Leftover::Abandoned, "x")
            .expect_err("無い予定");
        assert!(matches!(e, LedgerError::UnknownCapture), "{e:?}");
    }

    #[test]
    fn 予定を経ないテイクも閉じた予定と受領証を持つ() {
        let (mut l, rows, sid) = ledger();
        let id = l
            .commit_take(&FinalizedTake {
                row_id: rows[0].clone(),
                session_id: sid,
                rel_path: "audio/a_1.wav".into(),
                frames: 1,
                recorded_at: "2026-09-26T00:00:02Z".into(),
            })
            .expect("載せられる");
        let intents = l.intents().expect("読める");
        assert_eq!(intents.len(), 1);
        assert_eq!(intents[0].state, IntentState::Committed);
        assert_eq!(intents[0].take_id, Some(id));
        assert_eq!(l.receipts().expect("読める")[0].take_id, id);
        assert!(l.path_is_taken("audio/a_1.wav").expect("読める"));
        assert!(!l.path_is_taken("audio/a_2.wav").expect("読める"));
    }

    #[test]
    fn 開いている予定の行は組み直しで消さない() {
        let (mut l, rows, sid) = ledger();
        let capture = declare(&mut l, &rows[0], sid, "audio/a_1.wav");
        l.mark_leftover(&capture, Leftover::Orphaned, "2026-09-26T00:00:03Z")
            .expect("印を付けられる");
        let untaken: Vec<String> = l
            .untaken_rows(60, crate::db::RowOrigin::Preset)
            .expect("読める")
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert!(!untaken.contains(&rows[0]), "{untaken:?}");
        assert!(untaken.contains(&rows[1]), "{untaken:?}");

        // 閉じれば、ファイルを持たない行として組み直しの対象へ戻る。
        l.discard_capture(&capture, "2026-09-26T00:00:04Z")
            .expect("閉じられる");
        let untaken = l
            .untaken_rows(60, crate::db::RowOrigin::Preset)
            .expect("読める");
        assert!(untaken.iter().any(|(id, _)| id == &rows[0]));
    }
}
