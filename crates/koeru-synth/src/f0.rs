//! F0 推定の経路（`TR-SYN-22`）。
//!
//! **退避経路（Harvest）は二段構えにする**（`TR-SYN-22`）。
//!
//! 1. 最初の数テイクは **DIO + StoneMask** で即座に `.frq` を確定する
//! 2. 話者音域が判明して下限が引き上がったあと、**Harvest で静かに引き直す**
//!
//! `.frq` が要求するのは F0 と平均振幅だけなので、初期テイクの試唱には DIO の精度で足りる。
//! **待たせないことのほうが効く。**
//!
//! # 試唱と配布で条件を分ける
//!
//! **試唱は速度優先、`.frq` とパッケージ書き出しは品質優先**（`TR-SYN-22`）。
//! 同じ条件で回すと、試唱が遅いか、配布物が粗いかのどちらかになる。

use crate::world::{self, F0Method};

/// 話者音域が分かる前の探索下限（Hz）。**歌声の音域を広く取る。**
///
/// 目標音高から範囲を作ってはいけない。素材は別の音高で録られている。
pub const WIDE_FLOOR_HZ: f64 = 55.0;

/// 探索上限（Hz）。
pub const CEIL_HZ: f64 = 1100.0;

/// 引き直しの前に集める最小のテイク数（`TR-SYN-22`）。
///
/// **これだけ録れば、その人の音域がだいたい見える。**
pub const RANGE_SAMPLE_TAKES: usize = 5;

/// 話者音域から下限を引き上げるときの余裕（半音）。
///
/// **狭く取りすぎると、低い音を録ったときに範囲外へ落ちる。**
const FLOOR_MARGIN_SEMITONES: f64 = 5.0;

/// どちらの条件で解析するか（`TR-SYN-22`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// 試唱。**速度優先。** 待たせないことのほうが効く。
    Preview,
    /// `.frq` と配布パッケージ。**品質優先。**
    Distribution,
}

/// 解析の条件。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Conditions {
    /// 推定の手法。
    pub method: F0Method,
    /// 探索下限（Hz）。
    pub floor_hz: f64,
    /// 探索上限（Hz）。
    pub ceil_hz: f64,
    /// フレーム周期（ミリ秒）。
    pub frame_period_ms: f64,
}

/// 条件を決める（`TR-SYN-22`）。
///
/// `known_floor_hz` は話者音域から分かった下限。**まだ分からなければ `None`。**
#[must_use]
pub fn conditions(purpose: Purpose, known_floor_hz: Option<f64>) -> Conditions {
    let floor_hz = known_floor_hz.unwrap_or(WIDE_FLOOR_HZ).max(WIDE_FLOOR_HZ);
    match purpose {
        // **速度優先。** DIO は Harvest より速く、試唱には足りる。
        Purpose::Preview => Conditions {
            method: F0Method::DioStoneMask,
            floor_hz,
            ceil_hz: CEIL_HZ,
            frame_period_ms: world::DEFAULT_FRAME_PERIOD_MS,
        },
        // **品質優先。** 配るものはここで決まる。
        Purpose::Distribution => Conditions {
            method: F0Method::Harvest,
            floor_hz,
            ceil_hz: CEIL_HZ,
            frame_period_ms: world::DEFAULT_FRAME_PERIOD_MS,
        },
    }
}

/// 条件どおりに推定する。
#[must_use]
pub fn estimate(samples: &[f64], rate_hz: u32, c: &Conditions) -> (Vec<f64>, Vec<f64>) {
    world::estimate_f0(
        samples,
        rate_hz,
        c.method,
        c.floor_hz,
        c.ceil_hz,
        c.frame_period_ms,
    )
}

/// 集まったテイクの F0 から、探索の下限を引き上げる（`TR-SYN-22`）。
///
/// **テイクが足りないうちは引き上げない。** 1本や2本では音域は見えない。
///
/// 返るのは新しい下限。引き上げられなければ `None`。
#[must_use]
pub fn tighten_floor(observed_f0: &[Vec<f64>]) -> Option<f64> {
    if observed_f0.len() < RANGE_SAMPLE_TAKES {
        return None;
    }
    // 各テイクの有声フレームの最低値を集め、そのまた最低を取る。
    let lowest = observed_f0
        .iter()
        .filter_map(|f0| {
            f0.iter()
                .copied()
                .filter(|v| *v > 0.0)
                .fold(None::<f64>, |m, v| Some(m.map_or(v, |x| x.min(v))))
        })
        .fold(None::<f64>, |m, v| Some(m.map_or(v, |x| x.min(v))))?;

    // **余裕を持って下げる。** ぴったりに取ると、次に低い音を出したときに外れる。
    let floor = lowest * 2.0_f64.powf(-FLOOR_MARGIN_SEMITONES / 12.0);
    (floor > WIDE_FLOOR_HZ).then_some(floor)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **試唱は速度優先、配布は品質優先**（TR-SYN-22）。
    #[test]
    fn 試唱と配布で手法が違う() {
        assert_eq!(
            conditions(Purpose::Preview, None).method,
            F0Method::DioStoneMask,
            "試唱は速いほうを使う"
        );
        assert_eq!(
            conditions(Purpose::Distribution, None).method,
            F0Method::Harvest,
            "配るものは品質優先"
        );
    }

    /// **目標音高から範囲を作らない。** 素材は別の音高で録られている。
    #[test]
    fn 音域が分かる前は広く取る() {
        let c = conditions(Purpose::Preview, None);
        assert!((c.floor_hz - WIDE_FLOOR_HZ).abs() < f64::EPSILON);
        assert!((c.ceil_hz - CEIL_HZ).abs() < f64::EPSILON);
    }

    #[test]
    fn 音域が分かれば下限を上げる() {
        let c = conditions(Purpose::Distribution, Some(120.0));
        assert!((c.floor_hz - 120.0).abs() < f64::EPSILON);
    }

    /// **広い下限より下へは行かない。** 下げても得は無い。
    #[test]
    fn 下限は広い値より下がらない() {
        let c = conditions(Purpose::Preview, Some(20.0));
        assert!((c.floor_hz - WIDE_FLOOR_HZ).abs() < f64::EPSILON);
    }

    /// **テイクが足りないうちは引き上げない**（TR-SYN-22）。
    #[test]
    fn テイクが少ないうちは音域を決めない() {
        let few: Vec<Vec<f64>> = (0..RANGE_SAMPLE_TAKES - 1)
            .map(|_| vec![220.0, 230.0])
            .collect();
        assert_eq!(tighten_floor(&few), None);
    }

    #[test]
    fn 集まれば下限を引き上げる() {
        let takes: Vec<Vec<f64>> = (0..RANGE_SAMPLE_TAKES)
            .map(|i| vec![0.0, 200.0 + f64::from(i32::try_from(i).unwrap_or(0)) * 10.0])
            .collect();
        let floor = tighten_floor(&takes).expect("引き上げられること");
        // 最低 200Hz から 5半音ぶん下げた値。
        assert!(floor > WIDE_FLOOR_HZ);
        assert!(floor < 200.0, "余裕を持って下げること: {floor}");
        assert!(floor > 120.0, "下げすぎないこと: {floor}");
    }

    /// **有声フレームが無いテイクは無視する。**
    #[test]
    fn 無声だけのテイクは音域に効かない() {
        let mut takes: Vec<Vec<f64>> = (0..RANGE_SAMPLE_TAKES).map(|_| vec![220.0]).collect();
        takes.push(vec![0.0, 0.0]);
        let floor = tighten_floor(&takes).expect("引き上げられること");
        assert!(floor < 220.0 && floor > 120.0);
    }

    #[test]
    fn 低い声でも広い下限を割らない() {
        let takes: Vec<Vec<f64>> = (0..RANGE_SAMPLE_TAKES).map(|_| vec![60.0]).collect();
        // 60Hz から5半音下げると 45Hz。**広い下限を割るので引き上げない。**
        assert_eq!(tighten_floor(&takes), None);
    }
}
