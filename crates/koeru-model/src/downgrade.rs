//! 下位方式への書き出し（`TR-PKG-22`〜`25`, `DEC-RCL-007`）。
//!
//! 上下関係を宣言しない。 出せるかどうかは、その方式が要求するエイリアス表を
//! 既存素材が被覆しているかだけで決まる（[`crate::coverage`]）。
//!
//! # 独立した音源ルート・独立した ZIP
//!
//! 同一 ZIP へ同梱しない（`TR-PKG-25`）。 接頭辞を付ければ単独音として
//! 使えなくなり、付けなければ全 oto.ini 横断のエイリアス重複で Error になる。
//! 両立しない。
//!
//! # 値を流用しない
//!
//! `- CV` を素の `CV` として複製すると、語頭の子音区間を持ったままの oto が
//! 単独音として配られる（`TR-RCL-21`）。5値は対象方式の規約プリセットで
//! 再導出する（`TR-ALN-34`。`koeru-align` の `derive::rederive`）。
//!
//! # 声質は検知せず、推測材料も置かない
//!
//! 検知する手段が現在のスコープに無い。 一度は「跨いだ収録セッションの数と
//! 期間」を事実として出していたが、**落とした**（`DEC-RCL-015`）。
//! その数字から読めるのは「声が揃っていないかもしれない」だけで、
//! 判定しないと言いながら判断材料を置いていたことになる。

use std::collections::BTreeSet;

use crate::alias::Method;
use crate::inventory::UnitSet;
use crate::presamp::Rules;

use crate::coverage;

/// 書き出せない理由（`TR-PKG-23`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocked {
    /// 要求エイリアスが足りない。全件返す。
    ///
    /// 件数だけにしない。 「あと3件」だと、部分的なパッケージを出したくなる。
    Missing(Vec<String>),
    /// その方式の要求表を持たない。
    NoRequirementTable,
}

/// 下位方式の書き出し計画（`TR-PKG-24`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// 書き出す方式。
    pub method: Method,
    /// 複製する WAV の相対パス。
    ///
    /// **下位方式が参照するものだけ。** 全部複製すると、使わない WAV まで
    /// もう1本ぶんの容量を食う（`TR-PKG-24` の性能上の最適化）。
    pub wav_files: Vec<String>,
    /// 複製後の概算容量（バイト）。
    ///
    /// 「oto.ini 1ファイル分」ではない。 元パッケージとほぼ同等の容量が
    /// もう1本できる（`TR-PKG-24`）。
    pub bytes: u64,
}

/// 下位方式へ書き出せるか（`TR-PKG-23`）。
///
/// 100% 被覆のときだけ。 1件でも不足すれば実行せず、不足を全件返す。
///
/// # Errors
///
/// 不足があるとき、またはその方式の要求表が無いとき。
pub fn check(
    rules: &Rules,
    method: Method,
    set: UnitSet,
    provided: &BTreeSet<String>,
) -> Result<(), Blocked> {
    let Some(c) = coverage::coverage(rules, method, set, provided) else {
        return Err(Blocked::NoRequirementTable);
    };
    if c.is_complete() {
        Ok(())
    } else {
        Err(Blocked::Missing(c.missing))
    }
}

/// 書き出しに要る WAV とその容量から計画を作る（`TR-PKG-24`）。
///
/// `sources` は (相対パス, バイト数) の並び。 呼び出し側が台帳から引く。
///
/// **素材の由来は返さない**（`DEC-RCL-015`）。 跨いだセッション数や期間を出すと、
/// 声質の推測材料を置いたことになる。KOERU は声質に関与しない。
#[must_use]
pub fn plan(method: Method, sources: &[(String, u64)]) -> Plan {
    let mut wav_files: Vec<String> = sources.iter().map(|(p, _)| p.clone()).collect();
    wav_files.sort_unstable();
    wav_files.dedup();
    // 同じ WAV を2度数えない。 複数のエイリアスが1つのファイルを指す。
    let mut seen = BTreeSet::new();
    let bytes = sources
        .iter()
        .filter(|(p, _)| seen.insert(p.clone()))
        .map(|(_, b)| *b)
        .sum();
    Plan {
        method,
        wav_files,
        bytes,
    }
}

/// 下位方式パッケージの音源ルート名（`TR-PKG-24`, `TR-PKG-25`）。
///
/// 元の名前に方式を足した別の名前。 同じ名前にすると、受け取った側の
/// フォルダで上書きが起きる。
#[must_use]
pub fn root_name(original: &str, method: Method) -> String {
    let suffix = match method {
        Method::Single => "single",
        Method::Sequential => "vcv",
        Method::Cvvc => "cvvc",
    };
    format!("{original}-{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既定の綴り（`TR-SYN-36`）。
    fn core() -> Rules {
        Rules::builtin(UnitSet::Core)
    }

    use crate::inventory::units;

    fn sequential_full() -> BTreeSet<String> {
        coverage::required(&core(), Method::Sequential, UnitSet::Core).expect("表がある")
    }

    fn single_full() -> BTreeSet<String> {
        units(UnitSet::Core)
            .iter()
            .map(|u| u.kana.to_owned())
            .collect()
    }

    /// 100% 被覆のときだけ許す（`TR-PKG-23`）。
    #[test]
    fn 被覆していれば書き出せる() {
        assert_eq!(
            check(&core(), Method::Single, UnitSet::Core, &single_full()),
            Ok(())
        );
    }

    /// 不足は全件返す。件数だけにしない（`TR-PKG-23`）。
    #[test]
    fn 不足は全件返る() {
        let mut p = single_full();
        p.remove("あ");
        p.remove("か");
        let Err(Blocked::Missing(missing)) = check(&core(), Method::Single, UnitSet::Core, &p)
        else {
            panic!("足りないはず");
        };
        assert_eq!(missing.len(), 2);
        assert!(missing.contains(&"あ".to_owned()));
        assert!(missing.contains(&"か".to_owned()));
    }

    /// 連続音の素材は単独音へ降りられる（`TR-RCL-21`）。
    #[test]
    fn 連続音から単独音へ降りられる() {
        // 連続音の要求表は素の CV を第一候補に持たないので、
        // 降りるには語頭 CV を切り直すことになる。ここが見るのは可否だけ。
        let p = sequential_full();
        assert_eq!(
            check(&core(), Method::Sequential, UnitSet::Core, &p),
            Ok(())
        );
        assert_eq!(
            coverage::downgradable(&core(), UnitSet::Core, &p),
            [Method::Single],
            "構成上は出せる"
        );
    }

    /// 逆方向は出てこない（`INV-PKG-105`）。
    #[test]
    fn 単独音から連続音へは上がれない() {
        assert!(matches!(
            check(&core(), Method::Sequential, UnitSet::Core, &single_full()),
            Err(Blocked::Missing(_))
        ));
    }

    /// 同じ WAV を2度数えない（`TR-PKG-24`）。
    #[test]
    fn 容量は重複を数えない() {
        let sources = vec![
            ("a.wav".to_owned(), 100),
            ("a.wav".to_owned(), 100),
            ("b.wav".to_owned(), 50),
        ];
        let p = plan(Method::Single, &sources);
        assert_eq!(p.wav_files, ["a.wav", "b.wav"]);
        assert_eq!(p.bytes, 150);
    }

    /// 別の音源ルートになる（`TR-PKG-24`, `TR-PKG-25`）。
    #[test]
    fn 音源ルートは別の名前() {
        assert_eq!(root_name("koeru", Method::Single), "koeru-single");
        assert_ne!(root_name("koeru", Method::Cvvc), "koeru");
    }
}
