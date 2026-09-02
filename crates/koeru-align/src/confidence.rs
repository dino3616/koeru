//! 確信度（`TR-ALN-24`）。
//!
//! **どのエントリを人に見せるかを、これが決める。** 高ければ自動確定、低ければ確認キュー
//! （`TR-ALN-25`）。
//!
//! # 4成分
//!
//! | | 成分 | 何を見ているか |
//! |---|---|---|
//! | (1) | [`Confidence::path`] | 経路確信度。アライメントの解そのものの確からしさ |
//! | (2) | [`Confidence::sharpness`] | 境界鋭さ。境界がどれだけはっきり立っているか |
//! | (3) | [`Confidence::prior`] | 事前分布逸脱の裏返し。期待位置・集団中央値からのズレ |
//! | (4) | [`Confidence::acoustic`] | 音響異常度の裏返し。クリッピング・レベル不足・SNR |
//!
//! **どれも「高いほど良い」向きに揃えてある。** 逸脱と異常度は裏返して入れる。
//!
//! # 欠ける成分がある
//!
//! - **退避経路には (1) が無い**（`DEC-ALN-006`）。短時間パワーとゼロ交差率で境界を出すので、
//!   経路という概念を持たない。**0 を入れず `None` にする**——0 は「確信が無い」であって
//!   「測れない」ではない
//! - **単独音では (3) のグリッド由来の項が無い**（`TR-ALN-24` の [Fact]）。
//!   1ファイル1モーラなので拍の期待位置が存在しない。集団中央値からの逸脱だけが使える
//!
//! # 合成の重みに根拠が無い
//!
//! `TR-ALN-24` notes:
//!
//! > 4成分の合成重みを決める根拠がなく、内部評価ハーネス（`TR-ALN-32`）が
//! > 立ち上がるまで恣意的になる
//!
//! そして評価ハーネスは M6 へ回した（`DEC-ALN-007`）。**つまりこの合成式は、
//! 測って決めた形ではない。** 積を採っているのは「どれか1つでも低ければ疑う」を
//! 表すためで、それ以上の根拠は無い。**測れるようになったら差し替える。**

/// 確信度の4成分（`TR-ALN-24`）。
///
/// **成分ごとの値も保持する。** 合成スコアだけ持つと、
/// `TR-ALN-26` (3) の「低確信度の主因ラベル」が出せない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confidence {
    /// (1) 経路確信度。**退避経路は出せないので `None`。**
    pub path: Option<f64>,
    /// (2) 境界鋭さ。
    pub sharpness: f64,
    /// (3) 事前分布逸脱の裏返し。**外れ値でないほど高い。**
    pub prior: f64,
    /// (4) 音響異常度の裏返し。**異常が無いほど高い。**
    pub acoustic: f64,
}

/// 低確信度の主因（`TR-ALN-26` (3)）。
///
/// **用語を出さない言い換えは表示層が持つ。** ここが返すのはどの成分が落ちたかだけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// 経路確信度が低い。アライメントの解が競っている。
    Path,
    /// 境界が曖昧。
    Sharpness,
    /// 他のテイクと違う。
    Prior,
    /// 音が割れている / 小さすぎる。
    Acoustic,
}

impl Cause {
    /// 送信してよい固定文字列。**歌詞も音源名も混ぜない**（AGENTS.md #3）。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Path => "confidence.path",
            Self::Sharpness => "confidence.sharpness",
            Self::Prior => "confidence.prior",
            Self::Acoustic => "confidence.acoustic",
        }
    }
}

impl Confidence {
    /// 成分が全て満点の確信度。**試験と、成分を1つずつ組み立てるときの土台。**
    #[must_use]
    pub const fn full() -> Self {
        Self {
            path: Some(1.0),
            sharpness: 1.0,
            prior: 1.0,
            acoustic: 1.0,
        }
    }

    /// 合成スコア。**0.0〜1.0。**
    ///
    /// **積を採る。** どれか1つでも低ければ全体が落ちる、という形。
    /// 欠けている成分（退避経路の経路確信度）は掛けない——
    /// **測れないものを 0 として掛けると、退避経路が常に最低点になる。**
    ///
    /// **重みに根拠は無い**（`TR-ALN-24` notes、`DEC-ALN-007`）。
    #[must_use]
    pub fn score(&self) -> f64 {
        let mut s = self.sharpness * self.prior * self.acoustic;
        if let Some(p) = self.path {
            s *= p;
        }
        s.clamp(0.0, 1.0)
    }

    /// 成分が揃っているか。**退避経路では偽。**
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.path.is_some()
    }

    /// 低確信度の主因（`TR-ALN-26` (3)）。**いちばん低い成分を返す。**
    ///
    /// 全ての成分が `threshold` 以上なら `None`。
    #[must_use]
    pub fn cause(&self, threshold: f64) -> Option<Cause> {
        let mut worst: Option<(Cause, f64)> = None;
        let mut consider = |c: Cause, v: f64| {
            if v < threshold && worst.is_none_or(|(_, w)| v < w) {
                worst = Some((c, v));
            }
        };
        if let Some(p) = self.path {
            consider(Cause::Path, p);
        }
        consider(Cause::Sharpness, self.sharpness);
        consider(Cause::Prior, self.prior);
        consider(Cause::Acoustic, self.acoustic);
        worst.map(|(c, _)| c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 満点は一() {
        assert!((Confidence::full().score() - 1.0).abs() < 1e-9);
    }

    /// **欠けている成分を 0 として掛けない。**
    /// 掛けると退避経路が常に最低点になり、確信度が意味を失う。
    #[test]
    fn 経路確信度が無くてもスコアは落ちない() {
        let fallback = Confidence {
            path: None,
            ..Confidence::full()
        };
        assert!((fallback.score() - 1.0).abs() < 1e-9);
        assert!(!fallback.is_complete());
        assert!(Confidence::full().is_complete());
    }

    /// **どれか1つでも低ければ全体が落ちる。**
    #[test]
    fn 一つ低ければ全体が落ちる() {
        let c = Confidence {
            sharpness: 0.2,
            ..Confidence::full()
        };
        assert!((c.score() - 0.2).abs() < 1e-9);
    }

    #[test]
    fn 主因はいちばん低い成分() {
        let c = Confidence {
            path: Some(0.9),
            sharpness: 0.5,
            prior: 0.3,
            acoustic: 0.8,
        };
        assert_eq!(c.cause(0.6), Some(Cause::Prior));
    }

    /// **閾値を上回っていれば主因は無い。**
    #[test]
    fn 全て閾値以上なら主因は無い() {
        assert_eq!(Confidence::full().cause(0.6), None);
    }

    /// **退避経路では経路確信度が主因になりえない。** 測っていないので。
    #[test]
    fn 欠けた成分は主因にならない() {
        let c = Confidence {
            path: None,
            sharpness: 0.5,
            prior: 0.9,
            acoustic: 0.9,
        };
        assert_eq!(c.cause(0.6), Some(Cause::Sharpness));
    }

    #[test]
    fn 主因の種別は固定文字列() {
        for c in [Cause::Path, Cause::Sharpness, Cause::Prior, Cause::Acoustic] {
            assert!(c.kind().starts_with("confidence."));
        }
    }
}
