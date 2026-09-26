//! コールバックの中身のうち、OS の API に触らない部分。
//!
//! バックエンドのコールバックは、OS から受け取ったバッファをここへ渡すだけにする。
//! ここは CoreAudio なしで組み立つので、書いていない OS の組み立てでも、Miri でも試せる。
//!
//! どれも実時間のスレッドで走る。 確保も解放もロックもログもしない（`TR-REC-40`）。
//! 試験はそれを `alloc_guard` で数えて確かめる。

use std::sync::atomic::{AtomicU64, Ordering};

use crate::ring;

/// 全チャンネルを混ぜる（`TR-REC-06`）。
///
/// 既定にしない。 L+R の平均は、片側にしか信号が無いときに 6dB 損をする。
pub const MIX_ALL: usize = usize::MAX;

/// 二乗和を積むときの倍率。
const ENERGY_SCALE: f64 = 1_048_576.0;

/// チャンネルごとの二乗和。 校正で「有意な信号を持つ側」を選ぶために測る（`TR-REC-06`）。
///
/// f64 を CAS で積むとコールバックの中でループになるので、
/// 固定小数へ直して `fetch_add` する。 RMS の比較には十分な精度。
#[derive(Debug)]
pub(crate) struct ChannelEnergy {
    per_channel: Box<[AtomicU64]>,
    /// 上に積んだフレーム数。
    frames: AtomicU64,
}

impl ChannelEnergy {
    /// 確保はここで済ませる。 コールバックの中ではしない。
    pub(crate) fn new(channels: usize) -> Self {
        Self {
            per_channel: (0..channels).map(|_| AtomicU64::new(0)).collect(),
            frames: AtomicU64::new(0),
        }
    }

    /// 積んできたぶん全部の平均。 実時間の外で読む。
    pub(crate) fn rms(&self) -> Vec<f32> {
        let frames = self.frames.load(Ordering::Relaxed);
        if frames == 0 {
            return vec![0.0; self.per_channel.len()];
        }
        self.per_channel
            .iter()
            .map(|e| {
                let sum = e.load(Ordering::Relaxed) as f64 / ENERGY_SCALE;
                #[allow(clippy::cast_possible_truncation, reason = "RMS は 0.0..=1.0 付近")]
                let v = (sum / frames as f64).sqrt() as f32;
                v
            })
            .collect()
    }

    pub(crate) fn reset(&self) {
        for e in &self.per_channel {
            e.store(0, Ordering::Relaxed);
        }
        self.frames.store(0, Ordering::Relaxed);
    }
}

/// キャプチャの1周で、選んだ経路をリングへ流すための設定。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Route {
    pub(crate) channels: usize,
    /// どのチャンネルをモノラルの元にするか。 [`MIX_ALL`] なら全チャンネルの平均。
    pub(crate) source: usize,
    /// 収録中か。 止めている間もコールバックは来るので、ここで捨てる。
    pub(crate) armed: bool,
}

/// キャプチャの1周ぶんを受け取る。
///
/// `input` はチャンネルごとに `frames` 個ずつ並べたもの（非インターリーブ）。
/// 二乗和は収録していない間も積む——校正はストリームを開いたまま行うので、ここが唯一の経路。
/// 収録中なら、選んだチャンネル（または全チャンネルの平均）をリングへ流す（`TR-REC-06`）。
/// 入りきらなければ捨てて数える（[`ring::Producer::push_or_drop`]）。
///
/// 混ぜるときは `input` の先頭チャンネルの領域を作業場に使う。 ここでは確保しない。
pub(crate) fn deliver(
    input: &mut [f32],
    frames: usize,
    route: Route,
    energy: &ChannelEnergy,
    producer: &mut ring::Producer,
) {
    let channels = route.channels;
    let Some(input) = input.get_mut(..frames * channels) else {
        // 呼び出し側が大きさを確かめてから渡す。 足りなければ何も流さない。
        return;
    };

    for (ch, data) in input.chunks_exact(frames.max(1)).enumerate() {
        let sum: f64 = data.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "サンプルは -1.0..=1.0 付近。倍率を掛けても u64 に収まる"
        )]
        let scaled = (sum * ENERGY_SCALE) as u64;
        if let Some(slot) = energy.per_channel.get(ch) {
            slot.fetch_add(scaled, Ordering::Relaxed);
        }
    }
    energy.frames.fetch_add(frames as u64, Ordering::Relaxed);

    if !route.armed {
        return; // 収録していないので捨てる
    }

    // L+R の平均を既定にしない。片側にしか信号が無いときに 6dB 損をする。
    if route.source == MIX_ALL && channels > 1 {
        // 混ぜるのは、全チャンネルに有意な信号があると本人が選んだときだけ。
        let (out, rest) = input.split_at_mut(frames);
        for (i, slot) in out.iter_mut().enumerate() {
            let mut acc = *slot;
            for ch in 1..channels {
                acc += rest[(ch - 1) * frames + i];
            }
            *slot = acc / channels as f32;
        }
        producer.push_or_drop(out);
    } else {
        let ch = route.source.min(channels.saturating_sub(1));
        producer.push_or_drop(&input[ch * frames..(ch + 1) * frames]);
    }
}

/// 1回のレンダーで、リングから写したもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Filled {
    /// リングから写したフレーム数。残りは無音で埋めてある。
    pub(crate) frames: usize,
    pub(crate) end: End,
}

/// 埋めきれたか。 埋めきれなかったなら、なぜか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum End {
    /// 全部リングから埋まった。
    Full,
    /// もう継ぎ足さないと宣言されていて、末尾まで流し終えた。
    Done,
    /// まだ続きが来る予定なのに足りなかった。 枯渇として数える（`TR-SYN-03`）。
    Starved,
}

/// リングから `out` を埋める。 足りないぶんは無音にする。 再生のコールバックの本体。
///
/// `sealed` は**リングを読む前に**読んだ値を渡す。 継ぎ足しは書いてから閉じるので、
/// 閉じたのを見てから読めば、書いたものは全部見える。 読んだあとで見ると、
/// その間に書き足して閉じたぶんを残したまま、流し終えたことになる。
pub(crate) fn fill(consumer: &mut ring::Consumer, out: &mut [f32], sealed: bool) -> Filled {
    let frames = consumer.pop(out);
    // 埋めないと直前のバッファの中身が鳴る。
    out[frames..].fill(0.0);
    let end = if frames == out.len() {
        End::Full
    } else if sealed {
        End::Done
    } else {
        End::Starved
    };
    Filled { frames, end }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alloc_guard;

    /// 通し番号の音。 どこまで流れたかを値で見分ける。
    fn ramp(from: usize, n: usize) -> Vec<f32> {
        (from..from + n).map(|i| i as f32).collect()
    }

    fn route(channels: usize, source: usize) -> Route {
        Route {
            channels,
            source,
            armed: true,
        }
    }

    fn popped(c: &mut ring::Consumer) -> Vec<f32> {
        let mut out = vec![0.0_f32; 64];
        let n = c.pop(&mut out);
        out.truncate(n);
        out
    }

    // ## 再生

    #[test]
    fn 足りないぶんは無音で埋める() {
        let (mut p, mut c) = ring::channel(16);
        p.push(&[1.0, 2.0, 3.0]);
        let mut out = [9.0_f32; 5];
        let got = fill(&mut c, &mut out, true);
        assert_eq!(out, [1.0, 2.0, 3.0, 0.0, 0.0], "直前の中身を残さない");
        assert_eq!(got.frames, 3);
    }

    #[test]
    fn 埋めきれれば終わりでも枯渇でもない() {
        let (mut p, mut c) = ring::channel(16);
        p.push(&[1.0, 2.0, 3.0]);
        let mut out = [0.0_f32; 3];
        assert_eq!(
            fill(&mut c, &mut out, false),
            Filled {
                frames: 3,
                end: End::Full
            }
        );
    }

    /// 継ぎ足しが来る予定なら枯渇、もう来ないなら流し終えた（`TR-SYN-03`）。
    #[test]
    fn 足りないときは閉じていれば終わり閉じていなければ枯渇() {
        let (mut p, mut c) = ring::channel(16);
        let mut out = [0.0_f32; 4];

        p.push(&[1.0]);
        assert_eq!(fill(&mut c, &mut out, false).end, End::Starved);

        p.push(&[2.0]);
        assert_eq!(fill(&mut c, &mut out, true).end, End::Done);
    }

    /// 容量が2の冪でなくても、環をまたいで順に流れる（`DEC-REC-007`）。
    #[test]
    fn 容量が2の冪でなくても環をまたいで順に流れる() {
        let (mut p, mut c) = ring::channel(301); // **2の冪ではない。** 実効容量 300
        let mut out = vec![0.0_f32; 128];
        let mut written = 0;
        let mut read = 0;
        while read < 301 * 5 {
            written += p.push(&ramp(written, 97));
            let got = fill(&mut c, &mut out, false);
            for v in &out[..got.frames] {
                assert!(
                    (*v - read as f32).abs() < f32::EPSILON,
                    "{read} フレーム目で順序が壊れた: {v}"
                );
                read += 1;
            }
            assert!(out[got.frames..].iter().all(|v| *v == 0.0), "残りは無音");
        }
    }

    /// 埋めきる・枯渇する・流し終える、のどれでも確保しない（`TR-REC-40`）。
    #[test]
    fn 埋める経路は確保しない() {
        let (mut p, mut c) = ring::channel(11); // 2の冪ではない
        let block = ramp(0, 5);
        let mut out = [0.0_f32; 4];
        let (ends, n) = alloc_guard::count(|| {
            let mut ends = [0_usize; 3];
            // 3周で1組: 5 書いて 4 読む（埋まる）、残り 1 を読む（枯渇）、閉じて 0 を読む（終わり）。
            // 何組も回して環をまたがせる。
            for round in 0..21 {
                if round % 3 == 0 {
                    p.push(&block);
                }
                let f = fill(&mut c, &mut out, round % 3 == 2);
                ends[f.end as usize] += 1;
            }
            ends
        });
        assert_eq!(n, 0);
        assert!(
            ends.iter().all(|e| *e > 0),
            "3つの終わり方を全部通す: {ends:?}"
        );
    }

    // ## キャプチャ

    /// 既定は先頭チャンネル。 混ぜない（`TR-REC-06`）。
    #[test]
    fn 選んだチャンネルだけを流す() {
        let (mut p, mut c) = ring::channel(64);
        let energy = ChannelEnergy::new(2);
        let mut input = [1.0, 2.0, 3.0, -1.0, -2.0, -3.0]; // L, R の順に3フレームずつ
        deliver(&mut input, 3, route(2, 1), &energy, &mut p);
        assert_eq!(popped(&mut c), [-1.0, -2.0, -3.0]);
    }

    #[test]
    fn 範囲の外のチャンネルは最後のチャンネルに倒す() {
        let (mut p, mut c) = ring::channel(64);
        let energy = ChannelEnergy::new(2);
        let mut input = [1.0, 2.0, 5.0, 6.0];
        deliver(&mut input, 2, route(2, 7), &energy, &mut p);
        assert_eq!(popped(&mut c), [5.0, 6.0]);
    }

    #[test]
    fn 混ぜると選んだときだけ平均を流す() {
        let (mut p, mut c) = ring::channel(64);
        let energy = ChannelEnergy::new(3);
        let mut input = [3.0, 6.0, 0.0, 0.0, 0.0, 3.0];
        deliver(&mut input, 2, route(3, MIX_ALL), &energy, &mut p);
        assert_eq!(popped(&mut c), [1.0, 3.0]);
    }

    #[test]
    fn モノラルなら混ぜる指定でもそのまま流す() {
        let (mut p, mut c) = ring::channel(64);
        let energy = ChannelEnergy::new(1);
        let mut input = [0.25, 0.5];
        deliver(&mut input, 2, route(1, MIX_ALL), &energy, &mut p);
        assert_eq!(popped(&mut c), [0.25, 0.5]);
    }

    /// 収録していない間は流さないが、校正のための二乗和は積む。
    #[test]
    fn 収録していなくても二乗和は積む() {
        let (mut p, c) = ring::channel(64);
        let energy = ChannelEnergy::new(2);
        let mut input = [0.5, 0.5, 0.0, 0.0];
        let idle = Route {
            armed: false,
            ..route(2, 0)
        };
        deliver(&mut input, 2, idle, &energy, &mut p);
        assert!(c.is_empty(), "収録していないので流さない");
        let rms = energy.rms();
        assert!((rms[0] - 0.5).abs() < 1e-4, "{rms:?}");
        assert!(rms[1].abs() < 1e-6, "無音の側は 0: {rms:?}");

        energy.reset();
        assert_eq!(energy.rms(), [0.0, 0.0], "測り直せる");
    }

    /// 入りきらなければ捨てて数える。 待たない（`TR-REC-07`）。
    #[test]
    fn 入りきらなければ捨てて数える() {
        let (mut p, c) = ring::channel(4); // 実効容量 3
        let energy = ChannelEnergy::new(1);
        let mut input = [1.0; 5];
        deliver(&mut input, 5, route(1, 0), &energy, &mut p);
        assert_eq!(c.dropped(), 2);
    }

    #[test]
    fn 渡された領域が足りなければ何も流さない() {
        let (mut p, c) = ring::channel(64);
        let energy = ChannelEnergy::new(2);
        let mut input = [1.0; 3]; // 2ch × 2 フレームに足りない
        deliver(&mut input, 2, route(2, 0), &energy, &mut p);
        assert!(c.is_empty());
        assert_eq!(c.dropped(), 0, "取りこぼしとは数えない");
    }

    /// 選ぶ・混ぜる・止めている・捨てる、のどれでも確保しない（`TR-REC-40`）。
    #[test]
    fn キャプチャの経路は確保しない() {
        let (mut p, mut c) = ring::channel(301); // 2の冪ではない
        let energy = ChannelEnergy::new(2);
        let mut input = vec![0.1_f32; 2 * 128];
        let mut sink = vec![0.0_f32; 512];
        let ((), n) = alloc_guard::count(|| {
            for round in 0..12 {
                let source = [0, 1, MIX_ALL][round % 3];
                let r = Route {
                    armed: round % 4 != 3,
                    ..route(2, source)
                };
                deliver(&mut input, 128, r, &energy, &mut p);
                if round % 5 == 0 {
                    c.pop(&mut sink);
                }
            }
        });
        assert_eq!(n, 0);
        assert!(c.dropped() > 0, "捨てる経路も通っている");
    }
}
