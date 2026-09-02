//! サブフレーム補間（`TR-ALN-06`）。
//!
//! > oto の各値の出力時間分解能を 2ms 以下にする。**アライナのフレーム分解能が
//! > これより粗い場合は、境界近傍のフレーム事後確率を用いたサブフレーム補間で
//! > 境界位置を連続値として推定する**
//!
//! MFA のフレーム進み幅は 10ms（`EVID-ALN-001`）なので、そのままでは 5 倍粗い。
//!
//! # どう補間するか
//!
//! 境界の前後で、2つの音素の事後確率が入れ替わる。**入れ替わる点を線形で求める。**
//!
//! ```text
//!   P(前の音素)  ＼
//!                  ×  ← ここが境界
//!   P(次の音素)  ／
//!        t        t+1
//! ```
//!
//! 交点は `t + (a0 - b0) / ((a0 - b0) + (b1 - a1))`。**フレームの間を連続値で刺せる。**
//!
//! # 効かない場合がある
//!
//! `TR-ALN-06` notes:
//!
//! > 補間はフレーム事後確率が境界付近で単峰であることを仮定しており、
//! > **持続母音のような平坦な区間では機能しない可能性がある**
//!
//! 交点が見つからないときは、**Viterbi が出したフレーム境界をそのまま返す。**
//! 見つからないことを黙って埋めない——[`Interpolated::refined`] が
//! 補間できたかどうかを持つ。

/// 補間の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interpolated {
    /// 境界の位置（ミリ秒）。
    pub ms: f64,
    /// **事後確率から連続値を求められたか。**
    ///
    /// `false` なら [`Self::ms`] はフレームの刻みのまま（`TR-ALN-06` notes の
    /// 「平坦な区間では機能しない」場合）。
    pub refined: bool,
}

/// 補間の入力。**引数を並べると取り違える**ので、まとめて渡す。
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// フレーム × 音素の事後確率、行優先。
    pub posteriors: &'a [f32],
    /// 音素の数（行の幅）。
    pub phones: usize,
    /// フレーム数。
    pub frames: usize,
    /// Viterbi が出したフレーム境界（`after` が始まるフレーム）。
    pub frame: usize,
    /// 境界の手前の音素の列番号。
    pub before: usize,
    /// 境界の先の音素の列番号。
    pub after: usize,
    /// フレーム進み幅（ミリ秒）。
    pub hop_ms: f64,
    /// 交点を探す幅（フレーム）。**遠くの交点を拾うと別の境界のものを掴む。**
    pub window: usize,
}

/// 1つの境界を、事後確率から連続値で求める（`TR-ALN-06`）。
#[must_use]
pub fn refine(req: &Request<'_>) -> Interpolated {
    let &Request {
        posteriors,
        phones,
        frames,
        frame,
        before,
        after,
        hop_ms,
        window,
    } = req;
    #[allow(clippy::cast_precision_loss)]
    let fallback = Interpolated {
        ms: frame as f64 * hop_ms,
        refined: false,
    };
    if phones == 0 || frames == 0 || before >= phones || after >= phones {
        return fallback;
    }

    let at = |t: usize, p: usize| -> f64 {
        posteriors
            .get(t * phones + p)
            .map_or(0.0, |v| f64::from(*v))
    };

    let lo = frame.saturating_sub(window);
    let hi = (frame + window).min(frames.saturating_sub(1));

    // **境界の直近から外へ広げて探す。** いちばん近い交点を採る。
    let mut best: Option<(f64, f64)> = None; // (交点, 境界からの距離)
    for t in lo..hi {
        let (a0, b0) = (at(t, before), at(t, after));
        let (a1, b1) = (at(t + 1, before), at(t + 1, after));
        // 前のフレームで `before` が優勢、次で `after` が優勢なら、その間で入れ替わる。
        if a0 <= b0 || a1 >= b1 {
            continue;
        }
        let denom = (a0 - b0) + (b1 - a1);
        if denom <= f64::EPSILON {
            continue;
        }
        #[allow(clippy::cast_precision_loss)]
        let cross = t as f64 + (a0 - b0) / denom;
        #[allow(clippy::cast_precision_loss)]
        let dist = (cross - frame as f64).abs();
        if best.is_none_or(|(_, d)| dist < d) {
            best = Some((cross, dist));
        }
    }

    best.map_or(fallback, |(cross, _)| Interpolated {
        ms: cross * hop_ms,
        refined: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// フレーム × 2音素の事後確率を作る。
    fn post(rows: &[(f32, f32)]) -> Vec<f32> {
        rows.iter().flat_map(|(a, b)| [*a, *b]).collect()
    }

    /// **入れ替わりの点を、フレームの間で刺せる。**
    #[test]
    fn 交点をフレームの間で求められる() {
        // t=1 で 0.8 対 0.2、t=2 で 0.2 対 0.8。**ちょうど真ん中で入れ替わる。**
        let p = post(&[(1.0, 0.0), (0.8, 0.2), (0.2, 0.8), (0.0, 1.0)]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 4,
            frame: 2,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 3,
        });
        assert!(r.refined);
        assert!((r.ms - 15.0).abs() < 1e-6, "{}", r.ms);
    }

    /// **2ms 以下の分解能が出る**（`TR-ALN-06`）。
    #[test]
    fn 分解能がフレームより細かい() {
        let p = post(&[(0.9, 0.1), (0.55, 0.45), (0.1, 0.9)]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 3,
            frame: 2,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 3,
        });
        assert!(r.refined);
        // フレームの刻み（10ms / 20ms）のどちらとも違う位置に来る。
        assert!(r.ms > 10.0 && r.ms < 20.0, "{}", r.ms);
        assert!((r.ms % 10.0).abs() > 1e-6, "刻みに張り付いている: {}", r.ms);
    }

    /// **平坦なら補間しない**（`TR-ALN-06` notes）。
    /// 黙ってフレーム境界を「補間した」ことにしない。
    #[test]
    fn 平坦な区間では補間しない() {
        let p = post(&[(0.5, 0.5), (0.5, 0.5), (0.5, 0.5)]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 3,
            frame: 1,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 3,
        });
        assert!(!r.refined);
        assert!((r.ms - 10.0).abs() < 1e-9, "フレーム境界のまま");
    }

    /// **入れ替わりが無ければ補間しない。**
    #[test]
    fn 入れ替わりが無ければ補間しない() {
        let p = post(&[(0.9, 0.1), (0.9, 0.1), (0.9, 0.1)]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 3,
            frame: 1,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 3,
        });
        assert!(!r.refined);
    }

    /// **窓の外の交点は拾わない。** 別の境界のものを掴まないため。
    #[test]
    fn 窓の外は見ない() {
        // 交点は t=4 付近。境界は t=1、窓は 1。
        let p = post(&[
            (0.9, 0.1),
            (0.9, 0.1),
            (0.9, 0.1),
            (0.9, 0.1),
            (0.1, 0.9),
            (0.1, 0.9),
        ]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 6,
            frame: 1,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 1,
        });
        assert!(!r.refined);
    }

    /// **いちばん近い交点を採る。**
    #[test]
    fn 近いほうの交点を採る() {
        // 交点が t=0.5 と t=4.5 の2つ。境界 t=4 なら後ろを採る。
        let p = post(&[
            (0.9, 0.1),
            (0.1, 0.9),
            (0.9, 0.1),
            (0.9, 0.1),
            (0.9, 0.1),
            (0.1, 0.9),
        ]);
        let r = refine(&Request {
            posteriors: &p,
            phones: 2,
            frames: 6,
            frame: 4,
            before: 0,
            after: 1,
            hop_ms: 10.0,
            window: 5,
        });
        assert!(r.refined);
        assert!(r.ms > 40.0, "遠いほうを掴んでいる: {}", r.ms);
    }

    /// **範囲外の指定でも落ちない。**
    #[test]
    fn 範囲外でも落ちない() {
        let p = post(&[(1.0, 0.0)]);
        assert!(
            !refine(&Request {
                posteriors: &p,
                phones: 2,
                frames: 1,
                frame: 0,
                before: 0,
                after: 5,
                hop_ms: 10.0,
                window: 3,
            })
            .refined
        );
        assert!(
            !refine(&Request {
                posteriors: &p,
                phones: 0,
                frames: 0,
                frame: 0,
                before: 0,
                after: 1,
                hop_ms: 10.0,
                window: 3,
            })
            .refined
        );
    }
}
