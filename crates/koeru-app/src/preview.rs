//! 曲の試唱（`TR-SYN-01`〜`04`, `TR-SYN-18`, `TR-SYN-25`〜`27`, `TR-SYN-33`）。
//!
//! 押してから最初の音が鳴るまでを、曲全体の合成時間から切り離す（`TR-SYN-03`）。
//! 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

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
          引き当ては解決（`Song::resolve_by_tone`）が音高まで含めて済ませてあり
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

/// 進行中の試唱。落とすと止まる。
#[derive(Debug)]
pub struct Running {
    cancel: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    /// 中断する（`TR-SYN-27`）。
    ///
    /// 合図を立てて戻る。 合成の途中でも、次のフレーズの手前で抜ける。
    /// 中断済みフレーズの部分結果はキャッシュへ書かない。
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Release);
    }

    /// 止まるまで待つ。
    pub fn join(mut self) {
        self.cancel();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 合成した波形を受け取る口。
///
/// `push` は背後の合成スレッドだけが呼ぶ。 先へ溜めすぎないよう、受け取る側が
/// 空くまで待たせてよい。 待たせるなら、鳴らすのをやめたときに待ちを抜けさせる。
pub trait Sink: Send {
    fn push(&self, samples: &[f32]);
    /// もう来ないと伝える。
    fn seal(&self);
}

/// フレーズを順に合成して流す（`TR-SYN-03`）。
///
/// 1本目だけは同期で作る。 作れなければ、何も流さずにここで失敗を返す。
/// 流すのは背後のスレッドで、1本目から順に `sink` へ渡し、残りはそこで作る。
///
/// 1本目を呼び出し側で流さない。 `sink` は待たせてよい口なので、呼び出し側が
/// 画面の操作と同じロックを握ったまま流すと、止める操作まで待たされる。
/// 背後のスレッドが先に2本目の手前の休みを流して、順序が入れ替わることもなくなる。
///
/// # Errors
///
/// 先頭フレーズを合成できないとき。
#[tracing::instrument(skip(phrases, samples, cache, sink), fields(count = phrases.len()))]
pub fn start(
    phrases: Vec<(Phrase, f64)>,
    samples: Arc<dyn Samples + Send + Sync>,
    cache: Arc<Mutex<PhraseCache>>,
    sink: Box<dyn Sink>,
    rate_hz: u32,
) -> Result<Running, RenderError> {
    let cancel = Arc::new(AtomicBool::new(false));

    let mut rest = phrases;
    if rest.is_empty() {
        sink.seal();
        return Ok(Running {
            cancel,
            handle: None,
        });
    }
    let (first, lead) = rest.remove(0);
    let mut head = silence(lead, rate_hz);
    head.extend(render_cached(&first, samples.as_ref(), &cache, rate_hz)?);

    let handle = std::thread::spawn({
        let cancel = Arc::clone(&cancel);
        let cache = Arc::clone(&cache);
        move || {
            sink.push(&head);
            drop(head);
            for (p, lead) in rest {
                if cancel.load(Ordering::Acquire) {
                    // 部分結果を書かない（`TR-SYN-27`）。
                    break;
                }
                // 曲の休み（`TR-RCL-12`）。 鳴らさない時間もそのまま流す——
                // 詰めると、取り込んだ曲が元と違うリズムで鳴る。
                let pause = silence(lead, rate_hz);
                if !pause.is_empty() {
                    sink.push(&pause);
                }
                match render_cached(&p, samples.as_ref(), &cache, rate_hz) {
                    Ok(pcm) => sink.push(&pcm),
                    // 鳴らすのをやめるだけで、何も確定していない。
                    Err(e) => {
                        koeru_failure::record_failure(
                            &e,
                            koeru_failure::Outcome::NotCommitted,
                            "preview.render",
                        );
                        break;
                    }
                }
            }
            sink.seal();
        }
    });

    Ok(Running {
        cancel,
        handle: Some(handle),
    })
}

/// キャッシュを見てから合成する。
fn render_cached(
    phrase: &Phrase,
    samples: &dyn Samples,
    cache: &Mutex<PhraseCache>,
    rate_hz: u32,
) -> Result<Vec<f32>, RenderError> {
    let key = phrase.cache_key();
    if let Ok(mut c) = cache.lock()
        && let Some(hit) = c.get(key)
    {
        return Ok(to_f32(hit));
    }
    let pcm = render_phrase(phrase, samples, rate_hz)?;
    let out = to_f32(&pcm);
    if let Ok(mut c) = cache.lock() {
        c.put(key, pcm);
    }
    Ok(out)
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
}
