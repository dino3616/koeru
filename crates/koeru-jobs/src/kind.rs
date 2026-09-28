// JobKind — 仕事の種類（jobs.graphql の `JobKind` enum）。

use serde::{Deserialize, Serialize};

/// 仕事の種類。
///
/// GraphQL スカラ `JobKind` の列挙体実装。
/// 新規 job kind を足す場合は SDL と列挙体の両方を足す。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobKind {
    /// 解析（音素アライメント）
    Analysis,
    /// 合成（preview 用）
    PreviewRender,
    /// チャート事前計算
    Chart,
}

impl JobKind {
    /// 人間のわかりやすい名前を返す。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Analysis => "解析",
            Self::PreviewRender => "合成",
            Self::Chart => "チャート",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analysis_label() {
        assert_eq!(JobKind::Analysis.label(), "解析");
    }

    #[test]
    fn preview_render_label() {
        assert_eq!(JobKind::PreviewRender.label(), "合成");
    }

    #[test]
    fn chart_label() {
        assert_eq!(JobKind::Chart.label(), "チャート");
    }
}
