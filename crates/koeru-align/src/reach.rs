//! 方式別の到達水準（`TR-ALN-28`）。
//!
//! > 方式別の到達水準を宣言し実装で分岐させる。連続音・CVVC・多音階では
//! > 確認キューが空にならない前提とし、確認を「品質を上げる任意の工程」として扱い、
//! > 確認を飛ばしても書き出せる経路を必ず残す。
//! >
//! > [Unknown] 単独音について「確認キューが空になりうる」という宣言は、
//! > 確信度成分が実質3つしかない状態での宣言になるため、
//! > 内部評価ハーネスの実測が出るまで保留する
//!
//! # 単独音は「保留」のまま
//!
//! 評価ハーネスは M6 へ回した（`DEC-ALN-007`）。つまり `TR-ALN-28` が
//! 「実測が出るまで保留する」と書いた保留は、M6 まで解けない。
//!
//! [`Reach::Undeclared`] がその状態。「空になる」とも「ならない」とも言わない。
//! `Reach::MayEmpty` を単独音に割り当てるのは、測ってからにする。
//!
//! # 分岐は「確認を飛ばせるか」に効く
//!
//! 確認キューを空にできない方式では、1件ずつ確認させ続けると終わらない。
//! [`Reach::allows_skipping_review`] が真の方式では、まとめて確認
//! （`crate::review::ReviewQueue::confirm_all`）で抜ける経路を必ず出す。
//!
//! 飛ばしても書き出せる、は「確認せずに書き出せる」ではない。
//! `INV-ALN-003` は確認が残ったままの書き出しを禁じている。
//! 飛ばすというのは「1件ずつ見る代わりにまとめて引き受ける」こと。

use koeru_core::alias::Method;

/// 方式ごとの到達水準（`TR-ALN-28`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    /// 確認キューが空になりうるとは、まだ宣言しない。
    ///
    /// 単独音がここ。`TR-ALN-28` の [Unknown] が、評価ハーネス（M6）まで解けない。
    Undeclared,
    /// 確認キューは空にならない前提。 確認は品質を上げる任意の工程。
    NeverEmpty,
}

impl Reach {
    /// 送信してよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Undeclared => "reach.undeclared",
            Self::NeverEmpty => "reach.never_empty",
        }
    }

    /// 確認を飛ばせる経路を必ず出すか（`TR-ALN-28`）。
    ///
    /// 空にならない前提の方式では、飛ばす経路が無いと終われない。
    #[must_use]
    pub const fn allows_skipping_review(self) -> bool {
        matches!(self, Self::NeverEmpty)
    }

    /// 「編集画面を開かずに済む」と対外的に言えるか。
    ///
    /// どの方式でも偽。 単独音は保留（`TR-ALN-28`）、
    /// 連続音以上は空にならない前提。`TGT-ALN-007` が数値目標を置いていないのと同じ理由で、
    /// 根拠が実測に無い間は宣言しない（`DEC-ALN-008`）。
    #[must_use]
    pub const fn may_claim_no_editing(self) -> bool {
        false
    }
}

/// その方式の到達水準（`TR-ALN-28`）。
#[must_use]
pub const fn of(method: Method) -> Reach {
    match method {
        // 保留。 空になりうるかは評価ハーネスの実測まで言わない。
        Method::Single => Reach::Undeclared,
        // 連続音・CVVC は空にならない前提（`TR-ALN-28`）。
        Method::Sequential | Method::Cvvc => Reach::NeverEmpty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 単独音は保留のまま（`TR-ALN-28` の [Unknown]、`DEC-ALN-007`）。
    ///
    /// ここを `MayEmpty` に変えるのは、評価ハーネス（M6）の実測が出てから。
    #[test]
    fn 単独音の到達水準は保留() {
        assert_eq!(of(Method::Single), Reach::Undeclared);
    }

    /// 連続音以上は確認キューが空にならない前提（`TR-ALN-28`）。
    #[test]
    fn 連続音以上は空にならない前提() {
        assert_eq!(of(Method::Sequential), Reach::NeverEmpty);
        assert_eq!(of(Method::Cvvc), Reach::NeverEmpty);
    }

    /// 空にならない方式には、確認を飛ばす経路が要る（`TR-ALN-28`）。
    #[test]
    fn 空にならない方式は確認を飛ばせる() {
        assert!(of(Method::Sequential).allows_skipping_review());
        assert!(of(Method::Cvvc).allows_skipping_review());
    }

    /// どの方式でも「開かずに済む」とは言わない。
    /// 根拠が実測に無い（`TGT-ALN-007`、`DEC-ALN-008`）。
    #[test]
    fn 開かずに済むとは言わない() {
        for m in [Method::Single, Method::Sequential, Method::Cvvc] {
            assert!(!of(m).may_claim_no_editing());
        }
    }

    #[test]
    fn 種別は固定文字列() {
        for r in [Reach::Undeclared, Reach::NeverEmpty] {
            assert!(r.kind().starts_with("reach."));
        }
    }
}
