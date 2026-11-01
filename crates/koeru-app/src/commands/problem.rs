//! SDL の共有の失敗の型（`specs/application/schema/shared.graphql` の `Problem`・
//! `InvalidRequest`・`InputViolation`）を、刷新のあいだ tauri-specta で運ぶ形。
//!
//! tauri-specta は型を名前で書き出すので、同じ名前の型を2つ置くと bindings を作れない。
//! 操作ごとに写さず、ここに1つだけ置く。
//!
//! 分類・確定の状態・次の手の値は [`AppError`] と同じ小文字の語で運ぶ。 SDL の大文字の値へ
//! 揃えるのは GraphQL へ移すとき。

use serde::Serialize;

use koeru_failure::Class;

use crate::error::{AppError, FailureAction, FailureClass, FailureOutcome};

/// 予期できる失敗の共通の形（`Problem`）。 欄だけを持つ枝はこれを運ぶ。
///
/// 値として画面へ返すので、`AppError` の直列化を通らない。 記録するかどうかは呼び出し側が決める。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Problem {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
}

impl Problem {
    pub(crate) fn of(e: &AppError) -> Self {
        Self {
            code: e.code,
            class: e.class,
            commit: e.outcome,
            actions: vec![e.action],
            message: e.message.clone(),
        }
    }

    pub(crate) fn new(code: &'static str, class: Class, message: &'static str) -> Self {
        Self::of(&AppError::new(code, class, message))
    }
}

/// 入力が不正（`InvalidRequest`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct InvalidRequest {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub violations: Vec<InputViolation>,
}

impl InvalidRequest {
    pub(crate) fn of(p: Problem, violations: Vec<InputViolation>) -> Self {
        Self {
            code: p.code,
            class: p.class,
            commit: p.commit,
            actions: p.actions,
            message: p.message,
            violations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct InputViolation {
    /// 入力の中の場所。 欄の名前を . でつなぐ。
    pub field: &'static str,
    pub code: &'static str,
}
