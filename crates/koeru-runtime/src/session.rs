//! 一貫した読み（[`ProjectReadSession`]）と、そこから遅延解決する facet
//! （`DEC-PLT-044`）。
//!
//! # facet は台帳から作り直す。書き手が `Arc` で公開しない
//!
//! `DEC-PLT-043` の rationale は、`ReviewQueue` のような投影を書き手側から
//! 版つき `Arc` スナップショットとして公開する形を予告していた。 `DEC-PLT-044`
//! はこれを見直し、**台帳から導ける投影は、読みセッションの中でスナップショットから
//! 遅延に組み直す**（書き手が公開する不変の `Arc` は、台帳に無いもの——
//! 開いたときの検証結果（[`ProjectRuntime::open_report`](crate::ProjectRuntime::open_report)）
//! だけに限る）。
//!
//! 理由は、版と投影が構造的にずれえないこと。 書き手が公開する形だと
//! 「新しい書き込み経路を足したのに投影を更新し忘れる」という故障が机上では
//! 起こりうるが、スナップショットから毎回組み直せば、そもそも投影が版より
//! 古くなる経路が無い。
//!
//! # 遅延と使い回し
//!
//! facet は最初に呼ばれたときだけ組み立て、以後は同じ `Arc` を返す
//! （[`std::sync::OnceLock`]）。 組み立てが失敗しても書き込まない——次に
//! 呼んだときにもう一度試す。 これにより、1つの facet の構築が失敗しても
//! 他の facet は影響を受けない（T05 の adversarial「lazy facet failure」）。
//!
//! # まだ無い facet
//!
//! `editor`（M6 / T13 が持つ編集区間）と `distribution`（配布物の WAV 走査が
//! 要る。T08 / T09）はここに無い。 台帳の読みだけでは組めない・まだ設計が
//! 無いという理由でここに置いていないだけで、忘れているわけではない。

use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::Instant;

use koeru_core::capture;
use koeru_core::db::{LedgerSnapshot, Revision, RowTakes, Take};
use koeru_core::song::Song;
use koeru_model::review::ReviewQueue;

use crate::error::RuntimeError;
use crate::lease::ProjectLease;
use crate::runtime::ProjectRuntime;

/// 収録 facet（`specs/application/schema/recording.graphql` の
/// `RecordingFacet` の下敷き）。
///
/// [`LedgerSnapshot::rows_with_takes`] が返す、この版の全部の行とテイク。
#[derive(Debug)]
pub struct RecordingFacet {
    rows: Vec<RowTakes>,
}

impl RecordingFacet {
    /// 全部の行。 `RecordingFacet.rows` の窓はここから切り出す（T09）。
    #[must_use]
    pub fn rows(&self) -> &[RowTakes] {
        &self.rows
    }

    /// 行 ID から1件引く。 この版に無ければ `None`。
    #[must_use]
    pub fn row(&self, row_id: &str) -> Option<&RowTakes> {
        self.rows.iter().find(|r| r.row_id == row_id)
    }

    /// 全部の行が持つテイクから、ID で1件引く。
    ///
    /// 既に組み立てた `rows` の中だけを見る——単発の lookup だけなら
    /// [`ProjectReadSession::take`] のほうが軽い（`rows_with_takes` を
    /// 読まずに済む）。
    #[must_use]
    pub fn take(&self, take_id: i32) -> Option<&Take> {
        self.rows
            .iter()
            .flat_map(|r| &r.takes)
            .find(|t| t.id == take_id)
    }
}

/// 確認 facet（`specs/application/schema/review.graphql` の `ReviewFacet` の
/// 下敷き）。
///
/// [`crate::review::load`] が台帳（ここではスナップショット）から組み直した
/// キューと、鍵から書き戻す先のテイクへの表。
#[derive(Debug)]
pub struct ReviewFacet {
    pub queue: ReviewQueue,
    pub takes: std::collections::HashMap<String, i32>,
}

/// 曲目 facet（`specs/application/schema/repertoire.graphql` の
/// `RepertoireFacet` の下敷き）。
#[derive(Debug)]
pub struct RepertoireFacet {
    songs: Vec<(String, Song)>,
}

impl RepertoireFacet {
    /// 曲バンクの曲。 `(id, Song)` の組。
    #[must_use]
    pub fn songs(&self) -> &[(String, Song)] {
        &self.songs
    }
}

/// 1つの版に固定した読み（`ProjectLease` / 版 / facet）。
///
/// [`ProjectRuntime::read`](crate::ProjectRuntime::read) が返す。
/// GraphQL の1つの query が複数の facet を選んでも、同じ版から読む
/// （`docs/reports/architecture/08-graphql-application-contract.md` §5）。
///
/// `Send + Sync`。 GraphQL の resolver は並行に走りうるので、
/// スナップショットは内側で mutex に入れてある。
pub struct ProjectReadSession {
    snapshot: Mutex<Option<LedgerSnapshot>>,
    lease: ProjectLease,
    revision: Revision,
    open_report: Arc<capture::Report>,
    runtime: Weak<ProjectRuntime>,
    recording: OnceLock<Arc<RecordingFacet>>,
    review: OnceLock<Arc<ReviewFacet>>,
    repertoire: OnceLock<Arc<RepertoireFacet>>,
}

impl std::fmt::Debug for ProjectReadSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectReadSession")
            .field("lease", &self.lease)
            .field("revision", &self.revision)
            .finish_non_exhaustive()
    }
}

impl ProjectReadSession {
    /// [`ProjectRuntime::read`](crate::ProjectRuntime::read) だけが呼ぶ。
    pub(crate) fn new(
        snapshot: LedgerSnapshot,
        lease: ProjectLease,
        open_report: Arc<capture::Report>,
        runtime: Weak<ProjectRuntime>,
    ) -> Self {
        let revision = snapshot.revision();
        Self {
            snapshot: Mutex::new(Some(snapshot)),
            lease,
            revision,
            open_report,
            runtime,
            recording: OnceLock::new(),
            review: OnceLock::new(),
            repertoire: OnceLock::new(),
        }
    }

    /// このセッションが固定した版。
    #[must_use]
    pub const fn revision(&self) -> Revision {
        self.revision
    }

    /// このセッションを開いた貸与。
    #[must_use]
    pub const fn lease(&self) -> &ProjectLease {
        &self.lease
    }

    /// 開いたときの検証の結果（[`ProjectRuntime::open_report`](crate::ProjectRuntime::open_report) と同じ値）。
    #[must_use]
    pub fn open_report(&self) -> Arc<capture::Report> {
        Arc::clone(&self.open_report)
    }

    /// スナップショットへ1回アクセスする。
    ///
    /// セッションが生きている間、`snapshot` は常に `Some`——`close` に相当する
    /// 操作を外へ公開していないので、`None` になるのは [`Drop::drop`] の中だけ。
    fn with_snapshot<T>(
        &self,
        f: impl FnOnce(&mut LedgerSnapshot) -> Result<T, koeru_core::db::LedgerError>,
    ) -> Result<T, RuntimeError> {
        let mut guard = self.snapshot.lock().map_err(|_| RuntimeError::Poisoned)?;
        #[allow(
            clippy::expect_used,
            reason = "生きているセッションは close 経路（Drop 内）以外では常に Some を持つ"
        )]
        let snap = guard
            .as_mut()
            .expect("読み取り中のセッションは常にスナップショットを持つ");
        Ok(f(snap)?)
    }

    /// テイクを1件、スナップショットから直接引く。
    ///
    /// [`RecordingFacet`] を組まずに済む——`RecordingFacet.take(id)`
    /// （`recording.graphql`）は、既に組んである facet があればそちらを使い、
    /// 無ければこの経路を使う実装を想定している（T09）。
    ///
    /// # Errors
    ///
    /// スナップショットを読めない。
    pub fn take(&self, take_id: i32) -> Result<Option<Take>, RuntimeError> {
        self.with_snapshot(|s| s.take(take_id))
    }

    /// 収録 facet。 初めて呼んだときだけ `rows_with_takes` を読む。
    ///
    /// # Errors
    ///
    /// スナップショットを読めない。
    pub fn recording(&self) -> Result<Arc<RecordingFacet>, RuntimeError> {
        if let Some(f) = self.recording.get() {
            return Ok(Arc::clone(f));
        }
        let rows = self.timed(|| self.with_snapshot(LedgerSnapshot::rows_with_takes))?;
        let arc = Arc::new(RecordingFacet { rows });
        // 同時に2つの呼び出しが組み立てても、内容は同じ版から作った同じもの。
        // `get_or_init` が保証する「勝ったほうだけが残る」に任せる。
        Ok(Arc::clone(self.recording.get_or_init(|| arc)))
    }

    /// 確認 facet。 台帳から導ける投影なので、書き手側の `Arc` 公開を待たず
    /// ここで組み直す（`DEC-PLT-044`）。
    ///
    /// # Errors
    ///
    /// スナップショットを読めない。
    pub fn review(&self) -> Result<Arc<ReviewFacet>, RuntimeError> {
        if let Some(f) = self.review.get() {
            return Ok(Arc::clone(f));
        }
        let (queue, takes) = self.timed(|| self.with_snapshot(crate::review::load))?;
        let arc = Arc::new(ReviewFacet { queue, takes });
        Ok(Arc::clone(self.review.get_or_init(|| arc)))
    }

    /// 曲目 facet。
    ///
    /// # Errors
    ///
    /// スナップショットを読めない。
    pub fn repertoire(&self) -> Result<Arc<RepertoireFacet>, RuntimeError> {
        if let Some(f) = self.repertoire.get() {
            return Ok(Arc::clone(f));
        }
        let songs = self.timed(|| self.with_snapshot(LedgerSnapshot::songs_in_bank))?;
        let arc = Arc::new(RepertoireFacet { songs });
        Ok(Arc::clone(self.repertoire.get_or_init(|| arc)))
    }

    /// facet 1つの構築時間を span へ記録する（T05 の観測性契約
    /// 「snapshot construction duration」）。
    ///
    /// フィールドは版と経過時間だけ。 どの facet かは載せていない
    /// ——`crates/koeru-app/tests/offline.rs` のホワイトリストに無い名前を
    /// 増やさないため。
    fn timed<T>(&self, build: impl FnOnce() -> Result<T, RuntimeError>) -> Result<T, RuntimeError> {
        let span = tracing::info_span!(
            "facet_build",
            revision = self.revision.as_i64(),
            elapsed_ms = tracing::field::Empty
        );
        let _enter = span.enter();
        let start = Instant::now();
        let result = build();
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        span.record("elapsed_ms", elapsed_ms);
        result
    }
}

impl Drop for ProjectReadSession {
    /// 読み手をプールへ返す。 mutex が毒されている、実行時がもう無い、
    /// 実行時が閉じている、のどれかなら、読み手はここで（スコープの終わりで）
    /// そのまま drop する。
    fn drop(&mut self) {
        let Ok(mut guard) = self.snapshot.lock() else {
            return;
        };
        let Some(snapshot) = guard.take() else {
            return;
        };
        let reader = snapshot.close();
        if let Some(runtime) = self.runtime.upgrade() {
            runtime.return_reader(reader);
        }
    }
}
