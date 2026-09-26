//! 録音1回の保存の手順のうち、ファイルを見る側（`TR-REC-28`、`project-storage.fsl`）。
//!
//! 台帳の側の手は [`crate::db::intent`] にある。 WAV を書くのは `koeru-audio` で、
//! ここは書く場所を決めることと、落ちたあとに残ったものを見て予定へ印を付けることだけを持つ。
//!
//! **自動で直さない**（`TR-REC-31` の「不整合は提示して自動修復しない」）。 孤児を採ることも
//! 消すこともしない。 印を付けるのは、落ちたときに録音に使っていた予定だけで、
//! ファイルを持たないものは放棄の印を付けて残す（`project-storage.fsl` の ASSUME-8）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::db::{CaptureId, CaptureIntent, IntentState, Ledger, LedgerError, Leftover};

/// 保存の手順が失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// ファイルの有無を確かめられなかった。 `op` はどの操作かを示す固定の名前。
    #[error("録音の保存先を確かめられなかった")]
    Io {
        op: &'static str,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Ledger(#[from] LedgerError),
}

impl koeru_failure::Failure for CaptureError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "capture.io_failed",
            Self::Ledger(e) => e.code(),
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Io { source, .. } => koeru_failure::io_class(source),
            Self::Ledger(e) => e.class(),
        }
    }
}

type Result<T> = std::result::Result<T, CaptureError>;

fn io(op: &'static str) -> impl FnOnce(std::io::Error) -> CaptureError {
    move |source| CaptureError::Io { op, source }
}

/// 書きかけに付く印。 `koeru-audio` の `PartialTake` と同じ。
const PART_SUFFIX: &str = ".part";

/// 予定の場所に何があるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OnDisk {
    /// 確定した WAV がある。
    Finalized,
    /// 書きかけ（`.wav.part`）だけがある。
    Partial,
    /// どちらも無い。
    Nothing,
}

fn part_of(path: &Path) -> PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push(PART_SUFFIX);
    PathBuf::from(p)
}

/// プロジェクトの根からの相対パス。 台帳のテイクと予定はこの形で場所を持つ。
///
/// OS に依らず `/` 区切りに揃える。 揃えないと、Windows で書いた台帳を他 OS で
/// 開いたときに区切りがファイル名の一部と見なされ、また台帳の突き合わせが
/// スラッシュの向きで食い違う。
#[must_use]
pub fn rel_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.to_string_lossy().replace('\\', "/")
}

/// 予定の場所に何があるかを見る。
///
/// # Errors
///
/// 有無を確かめられない（権限など）。 無いことと確かめられないことを混ぜない——
/// 混ぜると、読めないだけの録音に放棄の印が付く。
pub fn inspect(root: &Path, rel: &str) -> Result<OnDisk> {
    let path = root.join(rel);
    if path.try_exists().map_err(io("inspect_final"))? {
        return Ok(OnDisk::Finalized);
    }
    if part_of(&path).try_exists().map_err(io("inspect_part"))? {
        return Ok(OnDisk::Partial);
    }
    Ok(OnDisk::Nothing)
}

/// 新しい録音を置く場所を決める。 `{row_id}_{世代}.wav` の世代を `first` から数え、
/// どのファイルもどの予定もどのテイクも持っていない最初の場所を返す。
///
/// **世代をテイクの数だけで決めていた。** コミットの前に落ちた孤児は数に入らないので、
/// 次の録音が同じ名前になり、確定の rename が孤児の WAV を上書きした
/// （`project-storage.fsl` の `FinalizedFilesOnlyLeaveByChoice` を破る）。
///
/// # Errors
///
/// 有無を確かめられない、台帳を読めない。
pub fn free_take_path(
    ledger: &mut Ledger,
    root: &Path,
    dir: &Path,
    row_id: &str,
    first: usize,
) -> Result<PathBuf> {
    let mut generation = first.max(1);
    loop {
        let path = dir.join(format!("{row_id}_{generation}.wav"));
        let free = !path.try_exists().map_err(io("probe_final"))?
            && !part_of(&path).try_exists().map_err(io("probe_part"))?
            && !ledger.path_is_taken(&rel_path(root, &path))?;
        if free {
            return Ok(path);
        }
        generation += 1;
    }
}

/// 録音に使っていた予定へ、場所に残ったものを見て印を付ける。 付けた印を返す。
/// 録音に使っていなければ何もせず `None`。
///
/// 確定の途中で失敗したとき（同じプロセスの中）と、落ちたあとの検証（[`verify`]）が呼ぶ。
/// 印を付けると予定は録音に使われなくなるので、次の録音の予定を書ける。
///
/// # Errors
///
/// 予定が無い、有無を確かめられない、台帳を書けない。
pub fn settle(
    ledger: &mut Ledger,
    root: &Path,
    capture: &CaptureId,
    at: &str,
) -> Result<Option<Leftover>> {
    let intent = ledger.intent(capture)?.ok_or(LedgerError::UnknownCapture)?;
    if intent.state != IntentState::Open {
        return Ok(None);
    }
    let mark = match inspect(root, &intent.rel_path)? {
        OnDisk::Finalized => Leftover::Orphaned,
        OnDisk::Partial => Leftover::Partial,
        OnDisk::Nothing => Leftover::Abandoned,
    };
    ledger.mark_leftover(capture, mark, at)?;
    Ok(Some(mark))
}

/// 台帳に載っていない確定済みの WAV（`REQ-REC-006` の孤児）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    /// プロジェクトの根からの相対パス。
    pub rel_path: String,
    /// その場所を持つ予定。 どの行・どの収録セッションの録音かはここから引く。
    ///
    /// `None` は予定を書く前の版で録ったもの。 名前から推し量らない。
    pub intent: Option<CaptureIntent>,
}

/// 起動時の検証の結果（`TR-REC-28`、`TR-REC-31`）。 提示するためのもので、何も直していない。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// 本人に採るか捨てるかを選ばせるもの。
    pub orphans: Vec<Orphan>,
    /// 書きかけだけを持って残った予定。 `.wav.part` はまだ採る手段が無いので、消さずに残す。
    pub partial: Vec<CaptureIntent>,
    /// この検証で放棄の印を付けた予定の数。 失われた録音は無いので本人には見せない。
    pub abandoned: usize,
}

/// 開いたときに、落ちたあとに残ったものを見て回る（`TR-REC-28`、`TR-REC-31`）。
///
/// 1. 落ちたときに録音に使っていた予定に、場所を見て印を付ける（[`settle`]）
/// 2. `audio_dir` の下で台帳に載っていない WAV を集め、同じ場所を持つ予定と突き合わせる
///
/// 同じプロセスで録音している間に呼ばない。 録音に使っている予定を、落ちたものとして扱う。
///
/// # Errors
///
/// 有無を確かめられない、`audio_dir` を読めない、台帳を読み書きできない。
pub fn verify(ledger: &mut Ledger, root: &Path, audio_dir: &Path, at: &str) -> Result<Report> {
    let mut report = Report::default();
    for intent in ledger.open_intents()? {
        if settle(ledger, root, &intent.capture, at)? == Some(Leftover::Abandoned) {
            report.abandoned += 1;
        }
    }

    let mut on_disk = Vec::new();
    collect_wavs(root, audio_dir, &mut on_disk)?;
    on_disk.sort();
    let by_path: BTreeMap<String, CaptureIntent> = ledger
        .intents()?
        .into_iter()
        .map(|i| (i.rel_path.clone(), i))
        .collect();
    report.orphans = ledger
        .find_orphans(&on_disk)?
        .into_iter()
        .map(|rel_path| Orphan {
            intent: by_path.get(&rel_path).cloned(),
            rel_path,
        })
        .collect();
    report.partial = ledger
        .open_intents()?
        .into_iter()
        .filter(|i| i.state == IntentState::Partial)
        .collect();

    let with_intent = report.orphans.iter().filter(|o| o.intent.is_some()).count();
    tracing::info!(
        kind = "orphan",
        count = report.orphans.len(),
        "孤児を数えた"
    );
    tracing::info!(
        kind = "orphan_with_intent",
        count = with_intent,
        "孤児を数えた"
    );
    tracing::info!(
        kind = "partial",
        count = report.partial.len(),
        "書きかけを数えた"
    );
    tracing::info!(
        kind = "abandoned",
        count = report.abandoned,
        "放棄の印を付けた"
    );
    Ok(report)
}

/// `dir` の下の確定済み WAV を、根からの相対パスで集める。 無ければ何もしない。
fn collect_wavs(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(io("read_audio_dir")(e)),
    };
    for entry in entries {
        let entry = entry.map_err(io("read_audio_dir"))?;
        let ty = entry.file_type().map_err(io("read_audio_dir"))?;
        let path = entry.path();
        if ty.is_dir() {
            collect_wavs(root, &path, out)?;
        } else if ty.is_file() && path.extension().is_some_and(|x| x == "wav") {
            out.push(rel_path(root, &path));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{NewIntent, SessionSnapshot};
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("koeru-capture-{}-{tag}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("audio")).expect("一時ディレクトリを作れること");
        d
    }

    fn ledger(root: &Path) -> (Ledger, Vec<String>, i32) {
        let mut l = Ledger::open(root.join("project.db")).expect("開ける");
        let list = generate_single(UnitSet::Core, 3).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        let sid = l
            .start_session(&SessionSnapshot {
                started_at: "2026-09-26T00:00:00Z".into(),
                device_id: "test".into(),
                sample_rate_hz: 44_100,
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

    #[test]
    fn 孤児の名前を次の録音に使わない() {
        let root = tmp("free");
        let (mut l, rows, _) = ledger(&root);
        let dir = root.join("audio");
        // 台帳に載らない確定済み WAV（孤児）と、書きかけ。
        std::fs::write(dir.join(format!("{}_1.wav", rows[0])), b"x").expect("書ける");
        std::fs::write(dir.join(format!("{}_2.wav.part", rows[0])), b"x").expect("書ける");
        let got = free_take_path(&mut l, &root, &dir, &rows[0], 1).expect("決められる");
        assert_eq!(got, dir.join(format!("{}_3.wav", rows[0])));
    }

    #[test]
    fn ファイルの無い予定の場所も使い直さない() {
        let root = tmp("free-intent");
        let (mut l, rows, sid) = ledger(&root);
        let dir = root.join("audio");
        let capture = CaptureId::generate();
        let rel = format!("audio/{}_1.wav", rows[0]);
        l.declare_capture(&NewIntent {
            capture: &capture,
            row_id: &rows[0],
            session_id: sid,
            rel_path: &rel,
            declared_at: "t",
        })
        .expect("書ける");
        l.mark_leftover(&capture, Leftover::Abandoned, "t")
            .expect("印を付けられる");
        let got = free_take_path(&mut l, &root, &dir, &rows[0], 1).expect("決められる");
        assert_eq!(got, dir.join(format!("{}_2.wav", rows[0])));
    }

    #[test]
    fn 場所に残ったもので印を選ぶ() {
        let root = tmp("settle");
        let (mut l, rows, sid) = ledger(&root);
        let mut captures = Vec::new();
        for (row, leave) in rows.iter().zip([Some("wav"), Some("wav.part"), None]) {
            let capture = CaptureId::generate();
            let rel = format!("audio/{row}_1.wav");
            l.declare_capture(&NewIntent {
                capture: &capture,
                row_id: row,
                session_id: sid,
                rel_path: &rel,
                declared_at: "t",
            })
            .expect("書ける");
            if let Some(ext) = leave {
                std::fs::write(root.join(format!("audio/{row}_1.{ext}")), b"x").expect("書ける");
            }
            captures.push(settle(&mut l, &root, &capture, "t").expect("見られる"));
        }
        assert_eq!(
            captures,
            [
                Some(Leftover::Orphaned),
                Some(Leftover::Partial),
                Some(Leftover::Abandoned)
            ]
        );
    }

    #[test]
    fn 予定の無い孤児は出どころを推し量らない() {
        let root = tmp("verify-legacy");
        let (mut l, _, _) = ledger(&root);
        std::fs::create_dir_all(root.join("audio/G3")).expect("作れる");
        std::fs::write(root.join("audio/G3/昔の_1.wav"), b"x").expect("書ける");
        let report = verify(&mut l, &root, &root.join("audio"), "t").expect("見られる");
        assert_eq!(report.orphans.len(), 1);
        assert_eq!(report.orphans[0].intent, None);
        assert!(report.orphans[0].rel_path.ends_with("昔の_1.wav"));
    }
}
