//! `cargo xtask check-profile <PROFILE-ID>`
//!
//! `KnowledgeSnapshot` の `Record` の口だけで書く。 `toml::Table` は触らない。

use super::CheckContext;
use crate::diagnostic::{Location, Report};
use crate::knowledge::{Id, Record};

pub(crate) fn check_profile(ctx: &CheckContext<'_>, rep: &mut Report) {
    // 引数が足りないときは入口が使い方を出して終わるので、ここには必ず1つ来る。
    let Some(profile_id) = ctx.args.first() else {
        return;
    };
    let snapshot = ctx.snapshot;

    let Some(profile) = snapshot
        .by_schema("profile")
        .find(|r| r.id().is_some_and(|id| id.as_str() == profile_id))
    else {
        rep.error_code(
            "profile.unknown-profile",
            format!("`{profile_id}` というプロファイルが無い"),
        );
        return;
    };

    let blocking: Vec<&Record> = snapshot
        .by_schema("question")
        .filter(|r| r.str("status") == Some("open"))
        .filter(|r| r.strs("blocks_profiles").iter().any(|p| p == profile_id))
        .collect();

    rep.note_code(
        "profile.summary",
        format!(
            "{profile_id}: FSL の要求 {} 件 / 決定 {} 件 / 予算 {} 件",
            profile.strs("includes_fsl").len(),
            profile.strs("decisions").len(),
            profile.strs("budgets").len()
        ),
    );

    for q in &blocking {
        let id = q.id().map_or("?", Id::as_str);
        let title = q.str("title").unwrap_or("");
        rep.error_at(
            "profile.blocked-by-open-question",
            Some(Location::new(q.provenance().path(), q.provenance().line())),
            format!("{id} が未決のまま {profile_id} を塞いでいる: {title}"),
        );
    }
}
