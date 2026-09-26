//! `cargo xtask check-profile <PROFILE-ID>`

use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, list_of, str_of, with_schema};

pub(crate) fn check_profile(entries: &[Entry], profile_id: &str, mut rep: Report) -> ExitCode {
    let Some(profile) =
        with_schema(entries, "profile").find(|e| str_of(&e.table, "id") == Some(profile_id))
    else {
        rep.error(format!("`{profile_id}` というプロファイルが無い"));
        return rep.finish("check-profile");
    };

    let blocking: Vec<&Entry> = with_schema(entries, "question")
        .filter(|e| str_of(&e.table, "status") == Some("open"))
        .filter(|e| {
            list_of(&e.table, "blocks_profiles")
                .iter()
                .any(|p| p == profile_id)
        })
        .collect();

    rep.note(format!(
        "{profile_id}: FSL の要求 {} 件 / 決定 {} 件 / 予算 {} 件",
        list_of(&profile.table, "includes_fsl").len(),
        list_of(&profile.table, "decisions").len(),
        list_of(&profile.table, "budgets").len()
    ));

    for q in &blocking {
        let id = str_of(&q.table, "id").unwrap_or("?");
        let title = str_of(&q.table, "title").unwrap_or("");
        rep.error(format!(
            "{id} が未決のまま {profile_id} を塞いでいる: {title}"
        ));
    }
    rep.finish("check-profile")
}
