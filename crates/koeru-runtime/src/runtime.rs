//! project の実行時（[`ProjectRuntime`]）。 単一の書き手と、読み手接続の
//! プールを持つ。
//!
//! **第二の `Studio` にしない**（`docs/reports/architecture/03-task-dag.md` の
//! T05 Architecture contract）。 ここが持つのは貸与・書き手・読み手プールの
//! 3つだけで、重い DSP・native の待ち・GraphQL / Tauri の処理は持たない
//! （`DEC-PLT-034` の実行域の表）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};

use koeru_core::capture;
use koeru_core::db::{Ledger, LedgerReader};
use koeru_core::project::ProjectDir;

use crate::error::RuntimeError;
use crate::lease::ProjectLease;
use crate::session::ProjectReadSession;
use crate::time::now_rfc3339;

/// 読み手接続のプールの上限。
///
/// **仮の数。** GraphQL の並行 query が実際に何本重なるかを実測していない
/// （T05 の「メモリ予算に触るものは実装着手前に数値を積み直す」対象。
/// `meta/questions/` へ論点を積む前段として、ここでは小さく倒してある）。
const READER_POOL_CAP: usize = 2;

/// project の実行時。
///
/// **`Arc` で配る。** [`ProjectReadSession`] が弱参照（[`Weak`]）で握り、
/// 読み終えた読み手をプールへ返すときに、この実行時がまだ生きているかを
/// 安全に判定する（生きていなければ、返さずにその読み手を drop する）。
pub struct ProjectRuntime {
    dir: ProjectDir,
    lease: ProjectLease,
    /// 開いたときの検証の結果（`koeru_core::capture::verify`）。 台帳には無い
    /// 値なので、書き手が版として持たず、開いた瞬間の不変な値として持ち回す
    /// （`DEC-PLT-044`。免れない不変の `Arc` は台帳に無いものだけに限ると決めた）。
    open_report: Arc<capture::Report>,
    writer: Mutex<Ledger>,
    readers: Mutex<Vec<LedgerReader>>,
    closed: AtomicBool,
    self_weak: Weak<ProjectRuntime>,
}

impl std::fmt::Debug for ProjectRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProjectRuntime")
            .field("lease", &self.lease)
            .field("closed", &self.closed.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl ProjectRuntime {
    /// 開く。 台帳を移行し、書き手の接続を開き、起動時の検証をしてから、
    /// 新しい貸与を1つ振る。
    ///
    /// 検証（[`capture::verify`]）が失敗しても開く——`Studio::open_project`
    /// （`crates/koeru-app/src/studio.rs`）と同じ縮退で、読めるものまで
    /// 読めなくしない。 失敗はその場で記録し、空の `Report` を持つ。
    ///
    /// **綴りの表の確認（presamp snapshot の ensure）はここではしない。**
    /// app 側のプリセットが要るので、書き手の側（`Studio::open_project` が
    /// `ProjectRuntime::writer` 経由で行う）に残す。
    ///
    /// # Errors
    ///
    /// 台帳を移行できない（[`RuntimeError::Project`]）、開けない
    /// （[`RuntimeError::Ledger`]）。
    #[tracing::instrument(skip(dir))]
    pub fn open(dir: ProjectDir) -> Result<Arc<Self>, RuntimeError> {
        dir.migrate_ledger()?;
        let mut writer = Ledger::open(dir.db_path())?;
        let report = capture::verify(&mut writer, dir.root(), &dir.audio_dir(), &now_rfc3339())
            .unwrap_or_else(|e| {
                koeru_failure::record_failure(
                    &e,
                    koeru_failure::Outcome::NotCommitted,
                    "capture.verify",
                );
                capture::Report::default()
            });
        let lease = ProjectLease::generate();
        tracing::info!(id = %lease, "project lease opened");
        Ok(Arc::new_cyclic(|weak| Self {
            dir,
            lease,
            open_report: Arc::new(report),
            writer: Mutex::new(writer),
            readers: Mutex::new(Vec::new()),
            closed: AtomicBool::new(false),
            self_weak: weak.clone(),
        }))
    }

    /// この実行時の貸与。 呼び出し側は以後、書き手と読みへの要求にこれを添える。
    #[must_use]
    pub const fn lease(&self) -> &ProjectLease {
        &self.lease
    }

    /// 開いている project の不変の識別子（`ProjectDir::id`）。
    #[must_use]
    pub fn project_id(&self) -> uuid::Uuid {
        self.dir.id()
    }

    /// プロジェクトのディレクトリ。 `Studio` が読み専用で使う経路向け。
    #[must_use]
    pub const fn dir(&self) -> &ProjectDir {
        &self.dir
    }

    /// 開いたときの検証の結果。
    #[must_use]
    pub fn open_report(&self) -> Arc<capture::Report> {
        Arc::clone(&self.open_report)
    }

    /// 渡された貸与がこの実行時のもので、かつ閉じていないことを見る。
    fn check_lease(&self, lease: &ProjectLease) -> Result<(), RuntimeError> {
        if *lease != self.lease || self.closed.load(Ordering::Acquire) {
            // 貸与の値そのものはランダムな UUID で、本人を指す情報を持たない
            // （`AGENTS.md` の禁止事項3のホワイトリストに沿う）。
            tracing::debug!(id = %lease, "expired lease used");
            return Err(RuntimeError::LeaseExpired);
        }
        Ok(())
    }

    /// 単一の書き手を取る。
    ///
    /// # 締め切りの罠
    ///
    /// 返す [`WriterGuard`] は `std::sync::Mutex` のロックそのもの。 同じ
    /// スレッドでもう一度 `writer()` を呼ぶ前に、前の guard を必ず drop する
    /// ——標準 mutex は再入可能ではないので、2つ目の呼び出しはそのスレッドの
    /// 中で永遠に待つ（自分自身との deadlock）。 短く持つ
    /// （`docs/reports/architecture/03-task-dag.md` の「short writer mutations」）。
    ///
    /// # Errors
    ///
    /// 貸与が違う・閉じている（[`RuntimeError::LeaseExpired`]）、mutex が
    /// 毒されている（[`RuntimeError::Poisoned`]）。
    pub fn writer(&self, lease: &ProjectLease) -> Result<WriterGuard<'_>, RuntimeError> {
        self.check_lease(lease)?;
        let guard = self.writer.lock().map_err(|_| RuntimeError::Poisoned)?;
        Ok(WriterGuard(guard))
    }

    /// 一貫した読み。 1つの版に固定した [`ProjectReadSession`] を返す。
    ///
    /// 読み手接続はプールから取る（無ければ新しく開く）。 セッションが読み終えたら
    /// プールへ返る（[`Self::return_reader`]）。
    ///
    /// # Errors
    ///
    /// 貸与が違う・閉じている（[`RuntimeError::LeaseExpired`]）、読み手を
    /// 開けない・スナップショットを取れない（[`RuntimeError::Ledger`]）、
    /// プールの mutex が毒されている（[`RuntimeError::Poisoned`]）。
    pub fn read(&self, lease: &ProjectLease) -> Result<ProjectReadSession, RuntimeError> {
        self.check_lease(lease)?;
        let reader = self.take_or_open_reader()?;
        let snapshot = reader.snapshot()?;
        Ok(ProjectReadSession::new(
            snapshot,
            *lease,
            Arc::clone(&self.open_report),
            self.self_weak.clone(),
        ))
    }

    fn take_or_open_reader(&self) -> Result<LedgerReader, RuntimeError> {
        let popped = {
            let mut pool = self.readers.lock().map_err(|_| RuntimeError::Poisoned)?;
            pool.pop()
        };
        match popped {
            Some(r) => Ok(r),
            None => Ok(Ledger::open_reader(self.dir.db_path())?),
        }
    }

    /// 読み終えた読み手を1つプールへ返す。 上限を超えるぶんは持たずに落とす。
    ///
    /// [`ProjectReadSession`] の drop から呼ぶ。 閉じた実行時には返さない——
    /// [`Self::close`] が既にプールを空にしているので、ここで足すとまた
    /// 読み手接続を持ったままになり、「読み手が全部いなくなってから移行する」
    /// という次の移行の前提を崩す。
    pub(crate) fn return_reader(&self, reader: LedgerReader) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        if let Ok(mut pool) = self.readers.lock()
            && pool.len() < READER_POOL_CAP
        {
            pool.push(reader);
        }
    }

    /// 閉じる。 読み手のプールを空にする。
    ///
    /// **読み手接続が1つも残っていないことは、次の移行の前提。** Windows は
    /// 開いたファイルの上へ rename できない（`AGENTS.md` の「書いていない OS
    /// 向けの組み立ても手元で通す」と同じ「実測するまで気づけない」種類の話で、
    /// T04d-1 が "Access is denied" で実際に踏んだ）。 `close` はこの実行時が
    /// 持つ読み手を確実に手放す。
    ///
    /// 同じ貸与でも二度目は [`RuntimeError::LeaseExpired`]（「閉じている」を
    /// 状態ではなく型で返す。 冪等に成功を返すと、呼び出し側は自分が本当に
    /// 閉じたのか、既に閉じていたのかを見分けられない）。
    ///
    /// 開いたままの [`ProjectReadSession`] はここでは終わらせない——読み取り
    /// トランザクションは接続ごとに独立しているので、握っているスナップショットを
    /// 読み終えるところまでは進める（`docs/reports/architecture/03-task-dag.md`
    /// の「lease close/open」）。
    ///
    /// # Errors
    ///
    /// 貸与が違う、もう閉じている（どちらも [`RuntimeError::LeaseExpired`]）。
    pub fn close(&self, lease: &ProjectLease) -> Result<(), RuntimeError> {
        if *lease != self.lease {
            return Err(RuntimeError::LeaseExpired);
        }
        // 「閉じていた」から「閉じた」への遷移を1回だけ許す。 同時に2つ来ても
        // どちらか一方だけが `false` を見る。
        if self.closed.swap(true, Ordering::AcqRel) {
            return Err(RuntimeError::LeaseExpired);
        }
        if let Ok(mut pool) = self.readers.lock() {
            pool.clear();
        }
        tracing::info!(id = %lease, "project lease closed");
        Ok(())
    }
}

/// 単一の書き手の guard。 [`Ledger`] へ deref する。
///
/// スコープを抜けるまで mutex を握る。 長生きさせない
/// （[`ProjectRuntime::writer`] のドキュメントにある締め切りの罠を参照）。
pub struct WriterGuard<'a>(MutexGuard<'a, Ledger>);

impl std::fmt::Debug for WriterGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WriterGuard { .. }")
    }
}

impl std::ops::Deref for WriterGuard<'_> {
    type Target = Ledger;

    fn deref(&self) -> &Ledger {
        &self.0
    }
}

impl std::ops::DerefMut for WriterGuard<'_> {
    fn deref_mut(&mut self) -> &mut Ledger {
        &mut self.0
    }
}
