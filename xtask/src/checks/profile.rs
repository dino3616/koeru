//! `cargo xtask check-profile <PROFILE-ID>`
//!
//! X03 の `KnowledgeSnapshot` へ載せ替えた最初の消費者。 `with_schema` / `str_of` /
//! `list_of` で `toml::Table` を直接触っていたのを、型つきの `Record` の口だけで書く。
//! 出力は1字も変えていない。

use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, Id, KnowledgeSnapshot, Record};

pub(crate) fn check_profile(entries: &[Entry], profile_id: &str, mut rep: Report) -> ExitCode {
    // 重複 ID の診断はここでは要らない——`check-profile` はプロファイルと問いの
    // 集まりしか見ず、重複の検出は `check-meta` の役目のまま（`dump_requirements` が
    // `requirements(entries).1` を捨てているのと同じ扱い）。
    let (snapshot, _) = KnowledgeSnapshot::from_entries(entries);

    let Some(profile) = snapshot
        .by_schema("profile")
        .find(|r| r.id().is_some_and(|id| id.as_str() == profile_id))
    else {
        rep.error(format!("`{profile_id}` というプロファイルが無い"));
        return rep.finish("check-profile");
    };

    let blocking: Vec<&Record> = snapshot
        .by_schema("question")
        .filter(|r| r.str("status") == Some("open"))
        .filter(|r| r.strs("blocks_profiles").iter().any(|p| p == profile_id))
        .collect();

    rep.note(format!(
        "{profile_id}: FSL の要求 {} 件 / 決定 {} 件 / 予算 {} 件",
        profile.strs("includes_fsl").len(),
        profile.strs("decisions").len(),
        profile.strs("budgets").len()
    ));

    for q in &blocking {
        let id = q.id().map_or("?", Id::as_str);
        let title = q.str("title").unwrap_or("");
        rep.error(format!(
            "{id} が未決のまま {profile_id} を塞いでいる: {title}"
        ));
    }
    rep.finish("check-profile")
}
