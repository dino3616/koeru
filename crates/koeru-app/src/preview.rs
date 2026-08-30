//! 曲の試唱（`TR-SYN-01`〜`04`, `TR-SYN-18`, `TR-SYN-25`〜`27`, `TR-SYN-33`）。
//!
//! **押してから最初の音が鳴るまでを、曲全体の合成時間から切り離す**（`TR-SYN-03`）。
//! 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。
//!
//! # 何をキャッシュするか
//!
//! **永続化するのは周波数表だけ**（`TR-SYN-25`）。スペクトル包絡と非周期性指標は
//! 持たない——音符ごとに必要な区間だけ算出する。フレーズ単位の合成済み波形は
//! メモリの LRU に置く。
//!
//! # いつ捨てるか
//!
//! 素材・oto・音符列・合成コアの版のどれかが変わったフレーズだけ（`TR-SYN-26`）。
//! **鍵にそれらが入っている**ので、変われば別の鍵になり、古い結果は自然に使われない。
//! **捨てるが、作り直すのは次に試唱されたとき。**

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use koeru_synth::phrase::{NoteSpec, Phrase, Samples, render_phrase};
use koeru_synth::resampler::RenderError;

/// メモリに置くフレーズの上限（`TR-SYN-25`）。
///
/// **上限を置かないと、長い曲を何度も試唱したときに際限なく伸びる。**
const CACHE_CAPACITY: usize = 64;

/// 鳴らしはじめる前に確保しておく長さ（ミリ秒、`TR-SYN-03`）。
///
/// **先行を保てない見込みのときは、途中で途切れさせるのではなく再生開始を遅らせる。**
/// 途切れる音は「自分の声だ」と認識する邪魔になる。
pub const LEAD_MS: f64 = 2000.0;

/// 連続して鳴らせる長さがこれに満たない曲は、試唱の選択肢に出さない（`TR-SYN-18` (3)）。
///
/// **[Unknown] この値に根拠はない**（`Q-SYN-001`）。
/// 「自分の声だ」と認識できる最短長は未検証で、ここが動けば
/// 必要な先頭項目数と課題曲設計が丸ごと変わる。
pub const MIN_PLAYABLE_MS: f64 = 4000.0;

/// フレーズ単位の合成結果を持つ（`TR-SYN-02`, `TR-SYN-25`）。
///
/// **使った順に古いものから捨てる。**
#[derive(Debug, Default)]
pub struct PhraseCache {
    entries: HashMap<u64, Vec<f64>>,
    /// 使った順。**末尾が最新。**
    order: Vec<u64>,
}

impl PhraseCache {
    /// 空のキャッシュ。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 入っていれば返す。**返したものが最新になる。**
    pub fn get(&mut self, key: u64) -> Option<&[f64]> {
        if !self.entries.contains_key(&key) {
            return None;
        }
        self.order.retain(|k| *k != key);
        self.order.push(key);
        self.entries.get(&key).map(Vec::as_slice)
    }

    /// 入れる。**上限を超えたら、いちばん古いものを捨てる。**
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
    /// エイリアスごとの素材の場所。
    pub paths: HashMap<String, PathBuf>,
    /// エイリアスごとの周波数表（`TR-SYN-08`, `TR-SYN-25`）。
    pub tables: HashMap<String, Vec<f64>>,
}

impl Samples for WavSamples {
    fn load(&self, note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError> {
        let path = self
            .paths
            .get(&note.alias)
            .ok_or(RenderError::RegionOutOfRange)?;
        let w = koeru_audio::wav::read(path).map_err(|_| RenderError::RegionOutOfRange)?;
        Ok((w.samples.iter().map(|s| f64::from(*s)).collect(), w.rate_hz))
    }

    fn frequency_table(&self, note: &NoteSpec) -> Vec<f64> {
        self.tables.get(&note.alias).cloned().unwrap_or_default()
    }
}

/// 進行中の試唱。**落とすと止まる。**
#[derive(Debug)]
pub struct Running {
    cancel: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Running {
    /// 中断する（`TR-SYN-27`）。
    ///
    /// **合図を立てて戻る。** 合成の途中でも、次のフレーズの手前で抜ける。
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
pub trait Sink: Send {
    /// 継ぎ足す。
    fn push(&self, samples: &[f32]);
    /// もう来ないと伝える。
    fn seal(&self);
}

/// フレーズを順に合成して流す（`TR-SYN-03`）。
///
/// **先頭フレーズができた時点で呼び出し側が鳴らしはじめられるよう、
/// 1本目だけは同期で作って返す。** 残りは背後で作る。
///
/// # Errors
///
/// 先頭フレーズを合成できないとき。
#[tracing::instrument(skip(phrases, samples, cache, sink), fields(count = phrases.len()), err)]
pub fn start(
    phrases: Vec<Phrase>,
    samples: Arc<dyn Samples + Send + Sync>,
    cache: Arc<Mutex<PhraseCache>>,
    sink: Box<dyn Sink>,
    rate_hz: u32,
) -> Result<(Vec<f32>, Running), RenderError> {
    let cancel = Arc::new(AtomicBool::new(false));

    let mut rest = phrases;
    if rest.is_empty() {
        sink.seal();
        return Ok((
            Vec::new(),
            Running {
                cancel,
                handle: None,
            },
        ));
    }
    let first = rest.remove(0);
    let head = render_cached(&first, samples.as_ref(), &cache, rate_hz)?;

    let handle = std::thread::spawn({
        let cancel = Arc::clone(&cancel);
        let cache = Arc::clone(&cache);
        move || {
            for p in rest {
                if cancel.load(Ordering::Acquire) {
                    // **部分結果を書かない**（TR-SYN-27）。
                    break;
                }
                match render_cached(&p, samples.as_ref(), &cache, rate_hz) {
                    Ok(pcm) => sink.push(&pcm),
                    Err(e) => {
                        tracing::warn!(kind = e.kind(), "フレーズを合成できなかった");
                        break;
                    }
                }
            }
            sink.seal();
        }
    });

    Ok((
        head,
        Running {
            cancel,
            handle: Some(handle),
        },
    ))
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

    /// **取り出したものが最新になる。**
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

    /// **閾値に根拠がないことを、値として固定しておく**（Q-SYN-001）。
    #[test]
    fn 最短長は要件どおりの暫定値() {
        assert!((MIN_PLAYABLE_MS - 4000.0).abs() < f64::EPSILON);
        assert!((LEAD_MS - 2000.0).abs() < f64::EPSILON);
    }
}
