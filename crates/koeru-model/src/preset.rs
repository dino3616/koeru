//! 方式プリセット（`TR-RCL-01`）。
//!
//! > プリセットが (a) 収録方式（CV / VCV / CVVC）、(b) 1行あたり最大モーラ数、
//! > (c) 音素インベントリのバージョン、(d) 語頭 CV を含めるか を持つ
//!
//! # 収録音高はここに持たない
//!
//! **方式と音高は別の選択**（`TR-RCL-01`）。 一度は「多音階連続音」という
//! 1個のプリセットとして持っていたが、そうすると本数と音高がこちらの決め打ちになり、
//! 本人が 2 本にすることも、C3 と A4 だけにすることもできない。
//!
//! 音高は録音リストの中身を変えない。 多音階は同じリストを音高の数だけ録る
//! （`TR-RCL-26`）。だから所要時間だけが本数に比例し、生成するリストは1本で足りる。
//!
//! # (f) は持たずに導く
//!
//! 同じ条文が続けてこう言っている。
//!
//! > 録音リスト・所要時間表示・カバレッジ台帳・書き出し可能方式の判定はすべて
//! > この1個のプリセットデータから導出し、同じ値を二重に持たない
//!
//! 所要時間はリストの行数とモーラ数から決まる。 値として持つと、
//! プリセットを直したときに片方だけが古くなる。ここが持つのは入力で、
//! 表示する値は [`MethodPreset::offer`] が作る。
//!

use crate::alias::Method;
use crate::inventory::{INVENTORY_VERSION, UnitSet};
use crate::pace::{self, MethodOffer};
use std::collections::BTreeSet;

use crate::presamp::Rules;
use crate::reclist::{
    DEFAULT_UNITS_PER_ROW, MAX_UNITS_PER_ROW, ReclistError, Row, generate_cvvc,
    generate_sequential, generate_single, repack,
};

/// 単音階の既定の収録音高（A3）。
pub const DEFAULT_TONE_MIDI: i32 = 57;

/// 1個の方式プリセット（`TR-RCL-01`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodPreset {
    /// プリセット ID。方式名ではなくこれで管理する。
    pub id: &'static str,
    /// 画面に出す名前。
    pub title: &'static str,
    /// (a) 収録方式。
    pub method: Method,
    /// (b) 1行あたり最大モーラ数。
    pub max_moras: usize,
    /// (c) 音素インベントリの版。プロジェクトは作成時の版を記録する（`TR-RCL-02`）。
    pub inventory_version: u32,
    /// 音素インベントリのセット。
    pub set: UnitSet,
    /// (d) 語頭 CV を含めるか。
    ///
    /// 連続音では常に真。 `TR-RCL-21` が「語頭 CV エイリアスを全 CV について
    /// 必ず含める」と要求していて、これが下位方式への書き出しを構成上保証している。
    pub include_head_cv: bool,
}

/// プリセットの不整合。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PresetError {
    /// 連続音なのに語頭 CV を落としている（`TR-RCL-21`）。
    #[error("連続音は語頭 CV を落とせない")]
    HeadCvRequired,
    /// リストを生成できない。
    #[error("録音リストを生成できない")]
    Reclist(#[from] ReclistError),
}

impl koeru_failure::Failure for PresetError {
    fn code(&self) -> &'static str {
        match self {
            Self::HeadCvRequired => "preset.head_cv_required",
            Self::Reclist(e) => e.code(),
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::HeadCvRequired => koeru_failure::Class::InvalidInput,
            Self::Reclist(e) => e.class(),
        }
    }
}

impl MethodPreset {
    /// 録音リストを生成する（`TR-RCL-03`, `TR-RCL-04`, `TR-RCL-05`）。
    ///
    /// 1音高ぶん。 多音階は同じリストを音高の数だけ録る（`TR-RCL-26`）。
    ///
    /// # Errors
    ///
    /// 語頭 CV を落とした連続音、または生成の条件を満たせないとき。
    pub fn reclist(&self, rules: &Rules) -> Result<Vec<Row>, PresetError> {
        if self.method == Method::Sequential && !self.include_head_cv {
            return Err(PresetError::HeadCvRequired);
        }
        Ok(match self.method {
            Method::Single => generate_single(self.set, self.max_moras)?,
            Method::Sequential => generate_sequential(self.set, self.max_moras)?,
            Method::Cvvc => generate_cvvc(rules, self.set, self.max_moras)?,
        })
    }

    /// 生成済みのリストから1件を作る（`TR-RCL-11`）。
    ///
    /// [`Self::offer`] と違い、リストを作り直さない。 呼び出し側が既に
    /// 持っているときに二度生成しないための入口。
    ///
    /// `tones` は収録音高の本数（`TR-RCL-01`）。 方式とは別の選択なので、
    /// プリセットは持たない——呼び出し側が本人の選んだ本数を渡す。
    #[must_use]
    pub fn offer_for(&self, rows: &[Row], tones: usize) -> MethodOffer {
        pace::offer(self.method, rows, tones)
    }

    /// 選択が要求するエイリアスだけを覆うリスト（`TR-RCL-16`, `DEC-RCL-011`）。
    ///
    /// # Errors
    ///
    /// 語頭 CV を落とした連続音、または生成の条件を満たせないとき。
    pub fn reclist_for(
        &self,
        rules: &Rules,
        required: &BTreeSet<String>,
    ) -> Result<Vec<Row>, PresetError> {
        if self.method == Method::Sequential && !self.include_head_cv {
            return Err(PresetError::HeadCvRequired);
        }
        Ok(repack(
            rules,
            self.method,
            self.set,
            required,
            self.max_moras,
        )?)
    }

    /// 方式選択画面に出す1件（`TR-RCL-11`）。
    ///
    /// 値として持たずに導く。 プリセットを直したときに片方だけが古くならない。
    ///
    /// # Errors
    ///
    /// リストを生成できないとき。
    pub fn offer(&self, rules: &Rules, tones: usize) -> Result<MethodOffer, PresetError> {
        Ok(pace::offer(self.method, &self.reclist(rules)?, tones))
    }
}

/// 同梱するプリセット（`TR-RCL-01`）。
///
/// 並びは所要時間の短い順。 画面はこの順を崩さない
/// ——毎回違う順に出ると、選び直すたびに読み直すことになる。
#[must_use]
pub fn builtin() -> Vec<MethodPreset> {
    vec![
        MethodPreset {
            id: "single",
            title: "単独音",
            method: Method::Single,
            max_moras: DEFAULT_UNITS_PER_ROW,
            inventory_version: INVENTORY_VERSION,
            set: UnitSet::Core,
            include_head_cv: false,
        },
        MethodPreset {
            id: "cvvc",
            title: "CVVC",
            method: Method::Cvvc,
            max_moras: MAX_UNITS_PER_ROW,
            inventory_version: INVENTORY_VERSION,
            set: UnitSet::Core,
            include_head_cv: true,
        },
        MethodPreset {
            id: "sequential",
            title: "連続音",
            method: Method::Sequential,
            max_moras: MAX_UNITS_PER_ROW,
            inventory_version: INVENTORY_VERSION,
            set: UnitSet::Core,
            include_head_cv: true,
        },
    ]
}

/// ID で引く。
#[must_use]
pub fn by_id(id: &str) -> Option<MethodPreset> {
    builtin().into_iter().find(|p| p.id == id)
}

#[cfg(test)]
mod tests {

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }
    use super::*;

    /// どのプリセットもリストを生成できる（`TR-RCL-01`）。
    #[test]
    fn 同梱プリセットはすべて生成できる() {
        for p in builtin() {
            let rows = p
                .reclist(&builtin_rules())
                .unwrap_or_else(|e| panic!("{}: {e}", p.id));
            assert!(!rows.is_empty(), "{}", p.id);
            assert!(
                rows.iter().all(|r| r.units.len() <= p.max_moras),
                "{} が上限を超える行を作る",
                p.id
            );
        }
    }

    /// 連続音は語頭 CV を落とせない（`TR-RCL-21`）。
    #[test]
    fn 連続音から語頭_cv_を落とせない() {
        let mut p = by_id("sequential").expect("ある");
        p.include_head_cv = false;
        assert_eq!(
            p.reclist(&builtin_rules()),
            Err(PresetError::HeadCvRequired)
        );
    }

    /// 音高の本数だけ時間が伸びる（`TR-RCL-09`）。
    ///
    /// **リストは同じ。** 多音階は同じリストを音高の数だけ録る（`TR-RCL-26`）ので、
    /// 本数はプリセットに要らない。
    #[test]
    fn 音高の本数だけ時間が伸びる() {
        let p = by_id("sequential").expect("ある");
        let one = p.offer(&builtin_rules(), 1).expect("出る");
        let three = p.offer(&builtin_rules(), 3).expect("出る");
        assert!((three.seconds - one.seconds * 3.0).abs() < 1e-6);
        assert_eq!(three.rows, one.rows, "リストは同じ。録る回数が3倍になる");
    }

    /// プリセットは方式だけを持つ（`TR-RCL-01`）。
    ///
    /// 音高は別の選択。 「多音階連続音」という1個のプリセットにすると、
    /// 本数も音高もこちらの決め打ちになる。
    #[test]
    fn プリセットは方式だけを持つ() {
        let ids: Vec<&str> = builtin().iter().map(|p| p.id).collect();
        assert_eq!(ids, ["single", "cvvc", "sequential"]);
        assert!(
            by_id("multi-pitch-sequential").is_none(),
            "音高では分けない"
        );
        assert!(by_id("なにこれ").is_none());
    }

    /// 並びは所要時間の短い順（`TR-RCL-11`）。
    #[test]
    fn 並びは所要時間の短い順() {
        let secs: Vec<f64> = builtin()
            .iter()
            .map(|p| p.offer(&builtin_rules(), 1).expect("出る").seconds)
            .collect();
        assert!(secs.windows(2).all(|w| w[0] <= w[1]), "{secs:?}");
    }
}
