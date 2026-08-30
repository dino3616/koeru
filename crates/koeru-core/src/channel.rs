//! 多チャンネル入力からモノラルを作る規則（`TR-REC-06`）。
//!
//! **L+R の平均を既定にしない。** 片側にしか信号が無いインタフェースは珍しくなく、
//! 平均すると 6dB 損をする。マイクを1本挿しただけの人が、
//! 理由の分からない小さい音で3時間録ることになる。
//!
//! **校正時に各チャンネルの RMS を測って、有意な信号を持つ1本を選ぶ。**
//! 全チャンネルに有意な信号があるときに限り、本人が「合成する」を選べる。
//!
//! ここは純粋な判断だけを持つ。測定は `koeru-audio` が行う。

/// これを下回るチャンネルは「信号が無い」とみなす。
///
/// **-60 dBFS。** これより小さいものは、挿していないか壊れている。
pub const SIGNIFICANT_RMS: f32 = 0.001;

/// 有意なチャンネルどうしで、この比を超えて開いていれば「片側だけ」とみなす。
///
/// **6dB。** 片方がもう片方の半分以下なら、混ぜる意味は無い。
const DOMINANCE_RATIO: f32 = 2.0;

/// どこからモノラルを作るか（`TR-REC-06`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// このチャンネルだけを使う。
    Channel(usize),
    /// 全チャンネルを混ぜる。**本人が選んだときだけ。**
    Mix,
}

/// 選び方の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// 既定として選んだもの。
    pub source: Source,
    /// **本人が「合成する」を選べるか**（`TR-REC-06`）。
    ///
    /// 全チャンネルに有意な信号があるときだけ真。
    pub may_mix: bool,
    /// 有意な信号を持つチャンネルの番号。
    pub significant: Vec<usize>,
}

/// チャンネルごとの RMS から、既定のモノラル化を決める（`TR-REC-06`）。
///
/// **有意な信号を持つチャンネルを1つ選ぶ。** 平均にしない。
#[must_use]
pub fn choose(rms: &[f32]) -> Choice {
    let significant: Vec<usize> = rms
        .iter()
        .enumerate()
        .filter(|(_, v)| **v >= SIGNIFICANT_RMS)
        .map(|(i, _)| i)
        .collect();

    // どれも信号が無い。**先頭に倒す。** 選びようが無いので、少なくとも一貫させる。
    if significant.is_empty() {
        return Choice {
            source: Source::Channel(0),
            may_mix: false,
            significant,
        };
    }

    // 一番大きいものを既定にする。
    let loudest = significant
        .iter()
        .copied()
        .max_by(|a, b| {
            rms.get(*a)
                .unwrap_or(&0.0)
                .partial_cmp(rms.get(*b).unwrap_or(&0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(0);

    // **全チャンネルに有意な信号があり、かつ大きく開いていないときだけ混ぜられる。**
    // 片側が半分以下なら、混ぜても損をするだけ。
    let all_significant = significant.len() == rms.len() && rms.len() > 1;
    let quietest = significant
        .iter()
        .filter_map(|i| rms.get(*i))
        .fold(f32::INFINITY, |m, v| m.min(*v));
    let balanced = rms
        .get(loudest)
        .is_some_and(|l| *l <= quietest * DOMINANCE_RATIO);

    Choice {
        source: Source::Channel(loudest),
        may_mix: all_significant && balanced,
        significant,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **L+R の平均を既定にしない**（TR-REC-06）。
    #[test]
    fn 片側にしか信号が無ければその側を選ぶ() {
        let c = choose(&[0.2, 0.000_01]);
        assert_eq!(c.source, Source::Channel(0));
        assert!(!c.may_mix, "混ぜる選択肢を出さない");
        assert_eq!(c.significant, [0]);

        let c = choose(&[0.000_01, 0.2]);
        assert_eq!(c.source, Source::Channel(1));
    }

    /// **全チャンネルに有意な信号があるときだけ混ぜられる**（TR-REC-06）。
    #[test]
    fn 両方に信号があれば混ぜる選択肢が出る() {
        let c = choose(&[0.2, 0.19]);
        assert!(c.may_mix);
        assert_eq!(c.source, Source::Channel(0), "既定は大きいほう");
        assert_eq!(c.significant, [0, 1]);
    }

    /// **大きく開いていれば混ぜない。** 6dB 差で片側扱い。
    #[test]
    fn 大きく開いていれば混ぜる選択肢を出さない() {
        let c = choose(&[0.2, 0.05]);
        assert!(c.significant.len() == 2, "どちらも有意ではある");
        assert!(!c.may_mix, "4倍も開いていれば混ぜる意味が無い");
        assert_eq!(c.source, Source::Channel(0));
    }

    #[test]
    fn 単一チャンネルは混ぜられない() {
        let c = choose(&[0.2]);
        assert_eq!(c.source, Source::Channel(0));
        assert!(!c.may_mix);
    }

    /// **どこにも信号が無くても先頭へ倒す。** 選びようが無いので一貫させる。
    #[test]
    fn 無音なら先頭に倒す() {
        let c = choose(&[0.0, 0.0]);
        assert_eq!(c.source, Source::Channel(0));
        assert!(!c.may_mix);
        assert!(c.significant.is_empty());
    }

    #[test]
    fn 三チャンネル以上でも選べる() {
        let c = choose(&[0.001, 0.3, 0.002]);
        assert_eq!(c.source, Source::Channel(1));
        assert_eq!(c.significant, [0, 1, 2], "閾値は超えている");
        assert!(!c.may_mix, "開きが大きい");
    }
}
