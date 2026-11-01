//! 曲の試唱（`TR-SYN-01`〜`04`, `TR-SYN-18`, `TR-SYN-25`〜`27`, `TR-SYN-33`）。
//!
//! 押してから最初の音が鳴るまでを、曲全体の合成時間から切り離す（`TR-SYN-03`）。
//! 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。 残りを流すのは
//! `JobRuntime` の試唱 lane の仕事で、解析の仕事を塞がない（`DEC-PLT-051`）。
//!
//! # 何をキャッシュするか
//!
//! 永続化するのは周波数表だけ（`TR-SYN-25`）。スペクトル包絡と非周期性指標は
//! 持たない——音符ごとに必要な区間だけ算出する。フレーズ単位の合成済み波形は
//! メモリの LRU に置く。
//!
//! # いつ捨てるか
//!
//! 素材・oto・音符列・合成コアの版のどれかが変わったフレーズだけ（`TR-SYN-26`）。
//! 鍵にそれらが入っているので、変われば別の鍵になり、古い結果は自然に使われない。
//! 捨てるが、作り直すのは次に試唱されたとき。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::Duration;

use koeru_failure::{Class, Failure};
use koeru_runtime::ProjectLease;
use koeru_runtime::jobs::{
    BasedOn, CancelProbe, CoalesceKey, Completed, ComputeError, JobError, JobId, JobKind,
    JobRuntime, JobSpec, OverflowPolicy, Priority, Receipt,
};
use koeru_synth::phrase::{NoteSpec, Phrase, Samples, render_phrase};
use koeru_synth::resampler::RenderError;

/// メモリに置くフレーズの上限（`TR-SYN-25`）。
///
/// 上限を置かないと、長い曲を何度も試唱したときに際限なく伸びる。
const CACHE_CAPACITY: usize = 64;

/// 鳴らしはじめる前に確保しておく長さ（ミリ秒、`TR-SYN-03`）。
///
/// 先行を保てない見込みのときは、途中で途切れさせるのではなく再生開始を遅らせる。
/// 途切れる音は「自分の声だ」と認識する邪魔になる。
pub const LEAD_MS: f64 = 2000.0;

/// 連続して鳴らせる長さがこれに満たない曲は、試唱の選択肢に出さない（`TR-SYN-18` (3)）。
///
/// [Unknown] この値に根拠はない（`Q-SYN-001`）。
/// 「自分の声だ」と認識できる最短長は未検証で、ここが動けば
/// 必要な先頭項目数と課題曲設計が丸ごと変わる。
pub const MIN_PLAYABLE_MS: f64 = 4000.0;

/// フレーズ単位の合成結果を持つ（`TR-SYN-02`, `TR-SYN-25`）。
///
/// 使った順に古いものから捨てる。
#[derive(Debug, Default)]
pub struct PhraseCache {
    entries: HashMap<u64, Vec<f64>>,
    /// 使った順。末尾が最新。
    order: Vec<u64>,
}

impl PhraseCache {
    /// 空のキャッシュ。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 入っていれば返す。返したものが最新になる。
    pub fn get(&mut self, key: u64) -> Option<&[f64]> {
        if !self.entries.contains_key(&key) {
            return None;
        }
        self.order.retain(|k| *k != key);
        self.order.push(key);
        self.entries.get(&key).map(Vec::as_slice)
    }

    /// 入れる。上限を超えたら、いちばん古いものを捨てる。
    pub fn put(&mut self, key: u64, samples: Vec<f64>) {
        if self.entries.insert(key, samples).is_none() {
            self.order.push(key);
        } else {
            self.order.retain(|k| *k != key);
            self.order.push(key);
        }
        while self.order.len() > CACHE_CAPACITY {
            let oldest = self.order.remove(0);
            self.entries.remove(&oldest);
        }
    }

    /// 入っている数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 空か。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// 素材を WAV から読む口。
#[derive(Debug)]
pub struct WavSamples {
    /// 素材ごとの周波数表（`TR-SYN-08`, `TR-SYN-25`）。**鍵はパス。**
    ///
    /// エイリアスで引かない。 多音階では音高ごとに同じエイリアスがあり
    /// （`TR-RCL-26`）、エイリアスを鍵にすると**1音高ぶんしか残らない。**
    /// どの音高の素材を使うかは `NoteSpec` が既に決めている。
    pub tables: HashMap<PathBuf, Vec<f64>>,
}

/// 合成へ渡す素材のサンプルレート（`TR-SYN-31`）。
///
/// この条件を満たさない WAV を、試唱のために変換して通さない。
/// 録音側の設定不備として扱う。ここで黙って変換すると、
/// 「なぜか音が変」の原因が試唱側に隠れる。
pub const REQUIRED_RATE_HZ: u32 = 44_100;

impl Samples for WavSamples {
    fn load(&self, note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError> {
        /*
          `NoteSpec` が持っているパスをそのまま読む。

          **エイリアスで引き直さない。** 多音階では音高ごとに同じエイリアスが
          あるので、エイリアスの表へ畳んだ時点で1音高ぶんしか残らない。
          引き当ては解決（`Song::resolve_by_cell`）が枠まで含めて済ませてあり
          （`TR-SYN-16`）、ここで引き直すとその判断が捨てられる。
        */
        let w = koeru_audio::wav::read(&note.sample_path)
            .map_err(|_| RenderError::SourceUnavailable)?;
        // 変換して通さない（`TR-SYN-31`）。
        if w.rate_hz != REQUIRED_RATE_HZ {
            tracing::warn!(
                got = w.rate_hz,
                want = REQUIRED_RATE_HZ,
                "素材のサンプルレートが合わない。録音側の設定不備として扱う"
            );
            return Err(RenderError::SampleRateMismatch);
        }
        Ok((w.samples.iter().map(|s| f64::from(*s)).collect(), w.rate_hz))
    }

    fn frequency_table(&self, note: &NoteSpec) -> Vec<f64> {
        // 素材と同じ鍵で引く。 別の音高の表を当てると、音高だけが飛ぶ。
        self.tables
            .get(&note.sample_path)
            .cloned()
            .unwrap_or_default()
    }
}

/// 1つの音符・1つの休みに置く長さの上限（ミリ秒）。
///
/// 取り込んだ曲の時間は外から来る（`TR-RCL-12`）。 極端に小さい BPM や
/// 巨大な `Length` を書いた UST は作れてしまうので、**長さをそのまま信じない。**
///
/// **`as usize` は桁あふれで 0 に落ちない。** 飽和して巨大な値になるので、
/// 確保に失敗してアプリごと落ちる。読めるファイルを試唱しただけで落ちるのは、
/// 取り込みの経路として成立しない。休みだけ抑えていて、歌う音符の長さは
/// 合成器まで素通りしていた——同じ理由なので、同じ上限を使う。
///
/// 30 秒。 1音を伸ばす長さとしても休みとしても十分に長く、無音なら確保は
/// 5MB 程度に収まる。
pub(crate) const MAX_SEGMENT_MS: f64 = 30_000.0;

/// 無音を作る（`TR-RCL-12` の休符）。
///
/// フレーズの手前に置く。 キャッシュの鍵に混ぜない——同じフレーズは、
/// 前の休みが何であっても同じ音。
fn silence(ms: f64, rate_hz: u32) -> Vec<f32> {
    // 有限でないものは 0 に倒す。 `NaN` は比較で常に偽になるので、
    // `clamp` に任せず先に畳む。
    let ms = if ms.is_finite() { ms } else { 0.0 };
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "上限を掛けたあとなので usize に収まる"
    )]
    let n = (ms.clamp(0.0, MAX_SEGMENT_MS) / 1000.0 * f64::from(rate_hz)) as usize;
    vec![0.0; n]
}

/// 試唱の仕事が止まるのを待つ上限（`TR-SYN-27`）。
///
/// **仮の値**（`Q-PLT-006`）。 待つのは、仕事が抱えている合成の写し（`Arc`）を、
/// プロジェクトを移るまでに手放させるため。 合成の1フレーズは途中で止められないので、
/// その1本が終わるまでは待つことになる。 上限は、それが固まったときに操作が返らなく
/// なるのを避けるためで、超えても仕事は取り消してある——戻れば何も書かずに終わる。
pub const STOP_WAIT: Duration = Duration::from_secs(5);

/// 試唱の仕事をまとめる鍵。 待ちにいる古い試唱は、新しい試唱に置き換わる。
const STREAM_KEY: &str = "preview_stream";

/// 試唱の仕事が終わった（走り終えた、または走らずに捨てられた）ことの印。
#[derive(Debug, Default)]
struct Done {
    finished: Mutex<bool>,
    cv: Condvar,
}

impl Done {
    fn finish(&self) {
        *self.finished.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.cv.notify_all();
    }

    /// 終わるまで、`limit` を上限に待つ。 終わっていれば `true`。
    fn wait(&self, limit: Duration) -> bool {
        let guard = self.finished.lock().unwrap_or_else(PoisonError::into_inner);
        let (guard, _) = self
            .cv
            .wait_timeout_while(guard, limit, |finished| !*finished)
            .unwrap_or_else(PoisonError::into_inner);
        *guard
    }
}

/// 落ちたら終わりを知らせる。 [`StreamJob`] の最後の欄に置く。
///
/// 欄は宣言順に落ちるので、合成の写し（`Arc`）を手放したあとで知らせることになる。
struct Finished(Arc<Done>);

impl Drop for Finished {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// 落ちたら「もう来ない」と伝える。 走り切っても、取り消されても、走らずに捨てられても。
struct Outlet(Box<dyn Sink>);

impl Drop for Outlet {
    fn drop(&mut self) {
        self.0.seal();
    }
}

/// 試唱の仕事が抱えるもの。 仕事の closure が丸ごと持ち、落ちると終わりが伝わる。
struct StreamJob {
    head: Vec<f32>,
    rest: Vec<(Phrase, f64)>,
    samples: Arc<dyn Samples + Send + Sync>,
    cache: Arc<Mutex<PhraseCache>>,
    rate_hz: u32,
    outlet: Outlet,
    _finished: Finished,
}

impl StreamJob {
    /// 先頭フレーズから順に流す。 区切りごとに取消の合図を見る（`TR-SYN-27`）。
    fn run(&mut self, probe: &CancelProbe) -> Result<(), ComputeError> {
        probe.check()?;
        // 先頭フレーズもここで流す。 呼び出し側で流すと、リングが満杯のあいだ
        // 画面の操作と同じロックを握ったまま待つ。
        let head = std::mem::take(&mut self.head);
        self.outlet.0.push(&head);
        drop(head);
        for (p, lead) in std::mem::take(&mut self.rest) {
            // 流し口は待たせる口。 待ちを抜けた直後に、合図を見る。
            probe.check()?;
            // 曲の休み（`TR-RCL-12`）。 鳴らさない時間もそのまま流す——
            // 詰めると、取り込んだ曲が元と違うリズムで鳴る。
            let pause = silence(lead, self.rate_hz);
            if !pause.is_empty() {
                self.outlet.0.push(&pause);
                probe.check()?;
            }
            let rendered =
                render_cached(&p, self.samples.as_ref(), &self.cache, self.rate_hz, || {
                    !probe.is_cancelled()
                });
            match rendered {
                Ok(Some(pcm)) => self.outlet.0.push(&pcm),
                Ok(None) => {
                    // 合成のあいだに取り消された。 `None` は合図が立っていたときだけなので、
                    // ここで抜ける。 部分結果は書いていない（`TR-SYN-27`）。
                    probe.check()?;
                    break;
                }
                // 鳴らすのをやめるだけで、何も確定していない。
                Err(e) => {
                    koeru_failure::record_failure(
                        &e,
                        koeru_failure::Outcome::NotCommitted,
                        "preview.render",
                    );
                    return Err(ComputeError::failed(&e));
                }
            }
        }
        Ok(())
    }
}

/// 進行中の試唱。落とすと止まる。
///
/// 流している間は `JobRuntime` の試唱 lane（`DEC-PLT-051`）の仕事。 取り消しも、
/// 止まるまでの待ちも、その仕事に対してする。
#[derive(Debug)]
pub struct Running {
    /// 取り消す相手。 流す音が無い試唱（フレーズが0本）は仕事を積まない。
    job: Option<(Arc<JobRuntime>, JobId)>,
    done: Arc<Done>,
}

impl Running {
    /// 何も流さずに終わっている試唱。
    fn finished() -> Self {
        let done = Arc::new(Done::default());
        done.finish();
        Self { job: None, done }
    }

    /// 中断する（`TR-SYN-27`）。
    ///
    /// 取り消して戻る。 待ちにいれば外れ、走っていれば合図を立てる。 合成の途中でも、
    /// 次のフレーズの手前で抜ける。 中断済みフレーズの部分結果はキャッシュへ書かない。
    pub fn cancel(&self) {
        if let Some((jobs, job)) = &self.job {
            // 終わっていれば何も起きない。 結果は要らない。
            let _ = jobs.cancel(*job);
        }
    }

    /// 止まるまで待つ。
    pub fn join(self) {
        drop(self);
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel();
        if !self.done.wait(STOP_WAIT) {
            tracing::warn!("試唱の仕事が上限の間に止まらなかった。取り消しは済んでいる");
        }
    }
}

/// 合成した波形を受け取る口。
///
/// `push` は試唱の仕事（作業スレッド）だけが呼ぶ。 先へ溜めすぎないよう、受け取る側が
/// 空くまで待たせてよい。 待たせるなら、鳴らすのをやめたときに待ちを抜けさせる。
pub trait Sink: Send {
    fn push(&self, samples: &[f32]);
    /// もう来ないと伝える。
    fn seal(&self);
}

/// 試唱を始められなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// 先頭のフレーズを合成できなかった。
    #[error(transparent)]
    Render(RenderError),
    /// 試唱の仕事を積めなかった（上限・貸与が閉じた・止めている最中）。
    #[error(transparent)]
    Job(JobError),
}

impl Failure for StartError {
    fn code(&self) -> &'static str {
        match self {
            Self::Render(e) => e.code(),
            Self::Job(e) => e.code(),
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Render(e) => e.class(),
            Self::Job(e) => e.class(),
        }
    }
}

/// 計算が済んだら、何も書かずに公開して仕事を終わらせる callback。
///
/// 合成の結果は台帳へ書かない（キャッシュはメモリ）。 公開しないと「公開待ち」のまま
/// 表に残る。 取消・期限・上限の失敗は、記録のほかにやることが無い。
///
/// `JobRuntime` は弱く持つ。 callback は待ちに残った仕事の closure の中にいるので、
/// 強く持つと `JobRuntime` が自分を生かし続けて止まらなくなる。
pub(crate) fn publish_nothing<T: Send + 'static>(
    jobs: &Arc<JobRuntime>,
) -> impl FnOnce(Result<Completed<T>, JobError>) + Send + 'static {
    let weak = Arc::downgrade(jobs);
    move |done| {
        if let (Ok(done), Some(runtime)) = (done, weak.upgrade()) {
            let _ = runtime.publish(done, &BasedOn::new(), |_| {
                Ok::<_, JobError>(((), Receipt::new(0)))
            });
        }
    }
}

/// フレーズを順に合成して流す（`TR-SYN-03`）。
///
/// 1本目だけは同期で作る。 作れなければ、何も流さずにここで失敗を返す。
/// 流すのは `JobRuntime` の試唱 lane の仕事（`DEC-PLT-051`）で、1本目から順に
/// `sink` へ渡し、残りはそこで作る。
///
/// 1本目を呼び出し側で流さない。 `sink` は待たせてよい口なので、呼び出し側が
/// 画面の操作と同じロックを握ったまま流すと、止める操作まで待たされる。
/// 仕事が先に2本目の手前の休みを流して、順序が入れ替わることもなくなる。
///
/// **期限は付けない。** 試唱は曲の長さの間ずっと流す仕事で、`sink` の空きを待つ時間も
/// 含めて長い。 期限を付けると、正常に流している最中の仕事が「固まった」と見なされ、
/// 代わりの作業スレッドが立つ。 止めるのは、取り消し（`TR-SYN-27`）だけ。
///
/// # Errors
///
/// 先頭フレーズを合成できないとき、仕事を積めないとき。
#[tracing::instrument(
    skip(jobs, lease, phrases, samples, cache, sink),
    fields(count = phrases.len())
)]
pub fn start(
    jobs: &Arc<JobRuntime>,
    lease: ProjectLease,
    phrases: Vec<(Phrase, f64)>,
    samples: Arc<dyn Samples + Send + Sync>,
    cache: Arc<Mutex<PhraseCache>>,
    sink: Box<dyn Sink>,
    rate_hz: u32,
) -> Result<Running, StartError> {
    let mut rest = phrases;
    if rest.is_empty() {
        sink.seal();
        return Ok(Running::finished());
    }
    let (first, lead) = rest.remove(0);
    let mut head = silence(lead, rate_hz);
    // 呼び出し側で作るので、取り消しの合図は無い。 常に残る。
    head.extend(
        render_cached(&first, samples.as_ref(), &cache, rate_hz, || true)
            .map_err(StartError::Render)?
            .unwrap_or_default(),
    );

    let done = Arc::new(Done::default());
    // 仕事が抱えて流す最初の塊の大きさ。
    let bytes = head.len().saturating_mul(std::mem::size_of::<f32>());
    let mut job = StreamJob {
        head,
        rest,
        samples,
        cache,
        rate_hz,
        outlet: Outlet(sink),
        _finished: Finished(Arc::clone(&done)),
    };
    let spec = JobSpec::new(
        lease,
        JobKind::PreviewRender,
        Priority::PreviewRequest,
        move |probe| job.run(probe),
    )
    .input_bytes(bytes)
    // 待ちにいる古い試唱は、新しい試唱に置き換わる。 走っている古い試唱は、
    // 呼び出し側が先に取り消す（`Running::cancel`）。
    .overflow(OverflowPolicy::Coalesce(CoalesceKey::new(STREAM_KEY)));
    let handle = jobs
        .submit_notify(spec, publish_nothing(jobs))
        .map_err(StartError::Job)?;

    Ok(Running {
        job: Some((Arc::clone(jobs), handle.job())),
        done,
    })
}

/// キャッシュを見てから合成する。
///
/// 合成が済んだ時点で `wanted` が偽なら、結果は捨てて `None` を返す。 キャッシュへ書かない
/// ——取り消されたフレーズの結果は、使われないまま残さない（`TR-SYN-27`）。
fn render_cached(
    phrase: &Phrase,
    samples: &dyn Samples,
    cache: &Mutex<PhraseCache>,
    rate_hz: u32,
    wanted: impl Fn() -> bool,
) -> Result<Option<Vec<f32>>, RenderError> {
    let key = phrase.cache_key();
    if let Ok(mut c) = cache.lock()
        && let Some(hit) = c.get(key)
    {
        return Ok(Some(to_f32(hit)));
    }
    let pcm = render_phrase(phrase, samples, rate_hz)?;
    if !wanted() {
        return Ok(None);
    }
    let out = to_f32(&pcm);
    if let Ok(mut c) = cache.lock() {
        c.put(key, pcm);
    }
    Ok(Some(out))
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "合成結果は -1.0..=1.0 付近。f32 で鳴らす"
)]
fn to_f32(x: &[f64]) -> Vec<f32> {
    x.iter().map(|v| *v as f32).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 使った順に古いものから捨てる() {
        let mut c = PhraseCache::new();
        for i in 0..u64::try_from(CACHE_CAPACITY).unwrap_or(64) + 10 {
            c.put(i, vec![0.0; 4]);
        }
        assert_eq!(c.len(), CACHE_CAPACITY, "上限を超えないこと");
        assert!(c.get(0).is_none(), "いちばん古いものが消えていること");
        assert!(
            c.get(CACHE_CAPACITY as u64 + 9).is_some(),
            "新しいものは残る"
        );
    }

    /// 取り出したものが最新になる。
    #[test]
    fn 取り出すと最新になる() {
        let mut c = PhraseCache::new();
        for i in 0..CACHE_CAPACITY as u64 {
            c.put(i, vec![0.0; 4]);
        }
        // 0 を触って最新にしてから、1つ足す。
        assert!(c.get(0).is_some());
        c.put(9999, vec![0.0; 4]);

        assert!(c.get(0).is_some(), "触った 0 は残る");
        assert!(c.get(1).is_none(), "代わりに 1 が消える");
    }

    #[test]
    fn 空のキャッシュ() {
        let mut c = PhraseCache::new();
        assert!(c.is_empty());
        assert!(c.get(1).is_none());
    }

    /// 閾値に根拠がないことを、値として固定しておく（`Q-SYN-001`）。
    #[test]
    fn 最短長は要件どおりの暫定値() {
        assert!((MIN_PLAYABLE_MS - 4000.0).abs() < f64::EPSILON);
        assert!((LEAD_MS - 2000.0).abs() < f64::EPSILON);
    }

    // ## 試唱の仕事（`DEC-PLT-051`）
    //
    // 本物の `JobRuntime`（既定の lane）と、本物の合成コアを使う。 素材と流し口だけ偽物で、
    // 偽の素材は決めた呼び出しで止める。 止める・進めるはチャネルで決め、`sleep` で待たない。

    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Instant;

    use koeru_core::oto::Oto;
    use koeru_runtime::jobs::{JobConfig, JobStatus};

    /// 事象が必ず来る待ちの上限。 来なければ試験が止まらずに落ちる。
    const LONG: Duration = Duration::from_secs(30);

    fn lease() -> ProjectLease {
        "00000000-0000-4000-8000-000000000001"
            .parse()
            .expect("UUID の形")
    }

    fn voiced(secs: f64) -> Vec<f64> {
        let n = (secs * 44_100.0) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / 44_100.0;
                // 倍音を足す。 純音では包絡が点になり、合成が音高を動かせない。
                (1..=8)
                    .map(|k| {
                        (2.0 * std::f64::consts::PI * 220.0 * f64::from(k) * t).sin() / f64::from(k)
                    })
                    .sum::<f64>()
                    * 0.2
            })
            .collect()
    }

    /// 1音だけのフレーズ。 `n` が違えば鍵が違う。
    fn phrase(n: u64) -> Phrase {
        Phrase::new(vec![NoteSpec {
            alias: "か".to_owned(),
            sample_path: PathBuf::from("x.wav"),
            sample_hash: Some(koeru_core::fingerprint::Digest::of(&n.to_le_bytes())),
            oto: Oto {
                offset_ms: 0.0,
                consonant_ms: 50.0,
                cutoff_ms: -480.0,
                preutterance_ms: 20.0,
                overlap_ms: 6.0,
            },
            midi: 60,
            duration_ms: 400.0,
        }])
    }

    fn phrases(count: u64) -> Vec<(Phrase, f64)> {
        (0..count).map(|n| (phrase(n), 0.0)).collect()
    }

    /// 呼ばれた回数を数え、決めた呼び出しで止める素材。
    struct Fake {
        source: Vec<f64>,
        loads: Arc<AtomicUsize>,
        /// 何番目（0 始まり）の呼び出しかを受け取る。 止めたいときはここで待つ。
        on_load: Box<dyn Fn(usize) + Send + Sync>,
    }

    impl Fake {
        fn new(on_load: impl Fn(usize) + Send + Sync + 'static) -> (Arc<Self>, Arc<AtomicUsize>) {
            let loads = Arc::new(AtomicUsize::new(0));
            let fake = Arc::new(Self {
                source: voiced(0.5),
                loads: Arc::clone(&loads),
                on_load: Box::new(on_load),
            });
            (fake, loads)
        }

        fn plain() -> (Arc<Self>, Arc<AtomicUsize>) {
            Self::new(|_| {})
        }
    }

    impl Samples for Fake {
        fn load(&self, _note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError> {
            let n = self.loads.fetch_add(1, Ordering::SeqCst);
            (self.on_load)(n);
            Ok((self.source.clone(), 44_100))
        }
    }

    /// 流した塊の長さと、封をしたかを覚える流し口。
    #[derive(Default)]
    struct Pipe {
        pushed: Mutex<Vec<usize>>,
        sealed: AtomicBool,
        /// `Playback` が止まった・落ちたことの代わり。 立つと、待っている `push` が抜ける。
        closed: AtomicBool,
        /// `n` 回目（1 始まり）の `push` で、`closed` が立つまで待つ。 0 なら待たない。
        stall_at: AtomicUsize,
    }

    impl Pipe {
        fn stalling_at(n: usize) -> Arc<Self> {
            let pipe = Arc::new(Self::default());
            pipe.stall_at.store(n, Ordering::SeqCst);
            pipe
        }

        fn pushes(&self) -> usize {
            self.pushed.lock().expect("lock").len()
        }

        fn close(&self) {
            self.closed.store(true, Ordering::SeqCst);
        }
    }

    struct TestSink(Arc<Pipe>);

    impl Sink for TestSink {
        fn push(&self, samples: &[f32]) {
            let nth = {
                let mut p = self.0.pushed.lock().expect("lock");
                p.push(samples.len());
                p.len()
            };
            if nth == self.0.stall_at.load(Ordering::SeqCst) {
                // 満杯のリングで待つ継ぎ足しの代わり。 `Playback` を落とすと抜ける。
                let end = Instant::now() + LONG;
                while !self.0.closed.load(Ordering::SeqCst) {
                    assert!(Instant::now() < end, "流し口が閉じられなかった");
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }

        fn seal(&self) {
            self.0.sealed.store(true, Ordering::SeqCst);
        }
    }

    fn begin(
        jobs: &Arc<JobRuntime>,
        phrases: Vec<(Phrase, f64)>,
        samples: Arc<Fake>,
        cache: &Arc<Mutex<PhraseCache>>,
        pipe: &Arc<Pipe>,
    ) -> Result<Running, StartError> {
        start(
            jobs,
            lease(),
            phrases,
            samples,
            Arc::clone(cache),
            Box::new(TestSink(Arc::clone(pipe))),
            44_100,
        )
    }

    fn runtime() -> Arc<JobRuntime> {
        JobRuntime::start(JobConfig::default())
    }

    fn cache() -> Arc<Mutex<PhraseCache>> {
        Arc::new(Mutex::new(PhraseCache::new()))
    }

    fn cached(cache: &Mutex<PhraseCache>, n: u64) -> bool {
        cache
            .lock()
            .expect("lock")
            .get(phrase(n).cache_key())
            .is_some()
    }

    fn job_id(running: &Running) -> koeru_runtime::jobs::JobId {
        running.job.as_ref().expect("仕事を積んでいる").1
    }

    /// 状態が必ずそうなる待ち。
    fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
        let end = Instant::now() + LONG;
        while !cond() {
            assert!(Instant::now() < end, "{what}");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn 試唱は先頭から順に流して封をし仕事は何も書かずに成功で終わる() {
        let jobs = runtime();
        let (samples, loads) = Fake::plain();
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));

        let running = begin(&jobs, phrases(3), samples, &cache, &pipe).expect("始められる");
        let job = job_id(&running);
        assert!(running.done.wait(LONG), "仕事が終わる");

        assert_eq!(pipe.pushes(), 3, "休みは無いので、フレーズ3本だけ流れる");
        assert!(pipe.sealed.load(Ordering::SeqCst), "最後に封をする");
        assert_eq!(loads.load(Ordering::SeqCst), 3);
        assert!(
            (0..3).all(|n| cached(&cache, n)),
            "できたフレーズはキャッシュへ入る"
        );
        // 公開しないと「公開待ち」のまま表に残る。
        eventually("仕事が成功で終わる", || {
            matches!(
                jobs.job(job).map(|j| j.status),
                Some(JobStatus::Succeeded { .. })
            )
        });
    }

    #[test]
    fn 先頭フレーズを合成できなければ何も流さず仕事も積まない() {
        struct Missing;
        impl Samples for Missing {
            fn load(&self, _note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError> {
                Err(RenderError::SourceUnavailable)
            }
        }
        let jobs = runtime();
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));

        let refused = start(
            &jobs,
            lease(),
            phrases(2),
            Arc::new(Missing),
            Arc::clone(&cache),
            Box::new(TestSink(Arc::clone(&pipe))),
            44_100,
        )
        .expect_err("先頭を作れない");

        assert!(matches!(
            refused,
            StartError::Render(RenderError::SourceUnavailable)
        ));
        assert_eq!(pipe.pushes(), 0, "何も流さない");
        assert_eq!(
            jobs.counts().pending + jobs.counts().running,
            0,
            "仕事を積まない"
        );
    }

    #[test]
    fn フレーズが無ければ封をして何も積まない() {
        let jobs = runtime();
        let (samples, _) = Fake::plain();
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));

        let running = begin(&jobs, Vec::new(), samples, &cache, &pipe).expect("始められる");

        assert!(pipe.sealed.load(Ordering::SeqCst));
        assert!(running.job.is_none());
        assert!(running.done.wait(Duration::ZERO), "もう終わっている");
        running.cancel();
    }

    #[test]
    fn 貸与が閉じていれば仕事を積めず失敗を返す() {
        let jobs = runtime();
        jobs.cancel_lease(&lease());
        let (samples, _) = Fake::plain();
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));

        let refused = begin(&jobs, phrases(2), samples, &cache, &pipe).expect_err("積めない");

        assert!(matches!(refused, StartError::Job(JobError::LeaseClosed)));
        assert_eq!(Failure::code(&refused), "jobs.lease_closed");
    }

    /// 止めると 200 ms 以内に仕事が終わり、次のフレーズは作らない（`TR-SYN-27`）。
    /// 流し口の空きを待っている仕事は、流し口が閉じれば抜ける。
    #[test]
    fn 止めると_200ms_以内に終わり次のフレーズを作らない() {
        let jobs = runtime();
        let (samples, loads) = Fake::plain();
        let (cache, pipe) = (cache(), Pipe::stalling_at(2));

        let running = begin(&jobs, phrases(4), samples, &cache, &pipe).expect("始められる");
        let job = job_id(&running);
        // 2本目を流し口へ渡したところで、空きを待っている。
        eventually("2本目まで流れる", || pipe.pushes() == 2);

        // 出力を落とす順（`PlaybackLease`）: 合図 → 再生を落とす → 合成を待つ。
        let asked = Instant::now();
        running.cancel();
        pipe.close();
        running.join();
        let took = asked.elapsed();

        assert!(took < Duration::from_millis(200), "止まるまで {took:?}");
        assert_eq!(loads.load(Ordering::SeqCst), 2, "3本目以降は作らない");
        assert!(pipe.sealed.load(Ordering::SeqCst));
        eventually("仕事が止められたと出る", || {
            matches!(
                jobs.job(job).map(|j| j.status),
                Some(JobStatus::Cancelled {
                    computation_stopped: true,
                    ..
                })
            )
        });
    }

    /// 合成の途中で止めたフレーズの結果は捨て、キャッシュへ書かない（`TR-SYN-27`）。
    #[test]
    fn 合成の途中で止めたフレーズはキャッシュへ書かず流さない() {
        let jobs = runtime();
        let (entered_tx, entered) = mpsc::channel();
        let (release, hold) = mpsc::channel::<()>();
        let hold = Mutex::new(hold);
        // 0 番目は先頭（呼び出し側で作る）。 1 番目の素材の読みで止める。
        let (samples, loads) = Fake::new(move |n| {
            if n == 1 {
                let _ = entered_tx.send(());
                let _ = hold.lock().expect("lock").recv_timeout(LONG);
            }
        });
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));

        let running = begin(&jobs, phrases(3), samples, &cache, &pipe).expect("始められる");
        let job = job_id(&running);
        entered.recv_timeout(LONG).expect("2本目の合成が始まる");
        running.cancel();
        let _ = release.send(());
        running.join();

        assert!(cached(&cache, 0), "先頭は完成していたので残る");
        assert!(!cached(&cache, 1), "止めたフレーズは書かない");
        assert!(!cached(&cache, 2), "その先は作らない");
        assert_eq!(pipe.pushes(), 1, "止めたフレーズは流さない");
        assert_eq!(loads.load(Ordering::SeqCst), 2);
        assert!(pipe.sealed.load(Ordering::SeqCst));
        eventually("仕事が止められたと出る", || {
            matches!(
                jobs.job(job).map(|j| j.status),
                Some(JobStatus::Cancelled {
                    computation_stopped: true,
                    ..
                })
            )
        });
    }

    /// 前の試唱の作業スレッドがまだ抜けていなくても、新しい試唱は待ちに並び、
    /// 古いのが抜けたあとで流れる（最新が勝つ）。
    #[test]
    fn 新しい試唱は古い試唱が抜けたあとで流れ古い方は取り消される() {
        let jobs = runtime();
        let (samples, _) = Fake::plain();
        let cache = cache();

        let old_pipe = Pipe::stalling_at(2);
        let old =
            begin(&jobs, phrases(3), Arc::clone(&samples), &cache, &old_pipe).expect("始められる");
        let old_job = job_id(&old);
        eventually("古い試唱が流し口で待つ", || {
            old_pipe.pushes() == 2
        });

        // 古い試唱を取り消し、まだ戻らないうちに新しい試唱を始める。
        old.cancel();
        let new_pipe = Arc::new(Pipe::default());
        let new = begin(&jobs, phrases(2), samples, &cache, &new_pipe).expect("始められる");
        assert_eq!(
            new_pipe.pushes(),
            0,
            "試唱の lane は1本。 古い試唱が抜けるまで流れない"
        );

        old_pipe.close();
        assert!(new.done.wait(LONG), "新しい試唱が流れ終わる");
        assert_eq!(new_pipe.pushes(), 2);
        assert!(new_pipe.sealed.load(Ordering::SeqCst));
        assert!(matches!(
            jobs.job(old_job).expect("表にある").status,
            JobStatus::Cancelled { .. }
        ));
    }

    /// 待ちにいる試唱を止めると、走らずに捨てられ、封をして終わりが伝わる。
    #[test]
    fn 待ちにいる試唱を止めると走らずに終わる() {
        let jobs = runtime();
        // 試唱の lane を、解析ではない別の長い仕事で塞ぐ。
        let (started_tx, started) = mpsc::channel();
        let (release, hold) = mpsc::channel::<()>();
        let blocker = jobs
            .submit(JobSpec::new(
                lease(),
                JobKind::PreviewRender,
                Priority::PreviewRequest,
                move |_| {
                    let _ = started_tx.send(());
                    let _ = hold.recv_timeout(LONG);
                    Ok(())
                },
            ))
            .expect("受け付けること");
        started.recv_timeout(LONG).expect("塞ぎが走り出す");

        let (samples, loads) = Fake::plain();
        let (cache, pipe) = (cache(), Arc::new(Pipe::default()));
        let running = begin(&jobs, phrases(3), samples, &cache, &pipe).expect("始められる");
        let job = job_id(&running);
        assert_eq!(loads.load(Ordering::SeqCst), 1, "先頭だけ呼び出し側で作る");

        running.cancel();

        assert!(
            running.done.wait(LONG),
            "走らずに捨てられても終わりが伝わる"
        );
        assert_eq!(pipe.pushes(), 0, "何も流れない");
        assert!(pipe.sealed.load(Ordering::SeqCst), "封はされる");
        assert_eq!(loads.load(Ordering::SeqCst), 1, "残りは作らない");
        assert!(matches!(
            jobs.job(job).expect("表にある").status,
            JobStatus::Cancelled { .. }
        ));
        let _ = release.send(());
        blocker.wait().expect("塞ぎは終わる");
    }

    /// 試唱が曲の長さの間ずっと走っても、前景の解析は待たずに終わる（`DEC-PLT-051`）。
    #[test]
    fn 試唱が流している間も前景の仕事は走って終わる() {
        let jobs = runtime();
        let (samples, _) = Fake::plain();
        let (cache, pipe) = (cache(), Pipe::stalling_at(2));
        let running = begin(&jobs, phrases(3), samples, &cache, &pipe).expect("始められる");
        eventually("試唱が流し口で待つ", || pipe.pushes() == 2);

        let done = jobs
            .submit(JobSpec::new(
                lease(),
                JobKind::Analysis,
                Priority::PostRecording,
                |_| Ok(7_u32),
            ))
            .expect("受け付けること")
            .wait_timeout(LONG)
            .expect("試唱を待たずに終わる")
            .expect("成功");
        assert_eq!(done.into_value(), 7);

        pipe.close();
        running.join();
    }

    #[test]
    fn 終わりの印は上限までに終わらなければ偽を返す() {
        let done = Done::default();
        assert!(!done.wait(Duration::from_millis(1)));
        done.finish();
        assert!(done.wait(Duration::ZERO));
    }
}
