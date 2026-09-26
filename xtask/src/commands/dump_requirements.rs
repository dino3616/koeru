//! `cargo xtask dump-requirements`

use std::process::ExitCode;

use crate::knowledge::{Entry, list_of, requirements};

/// 要件の登録簿を書き出す。 移行の照合用。
///
/// US(0x1f) 区切りのフィールド、RS(0x1e) 区切りのレコード。 読み込みの段の失敗は
/// 報告しない——報告を持たないコマンドとして書かれていた形をそのまま残している。
pub(crate) fn dump_requirements(entries: &[Entry]) -> ExitCode {
    for (id, t) in requirements(entries).0 {
        let get = |k: &str| {
            t.get(k)
                .and_then(toml::Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let mut fields = vec![id, get("title"), get("confidence")];
        fields.push(list_of(&t, "depends_on").join(","));
        fields.push(get("statement"));
        fields.extend(list_of(&t, "notes"));
        print!("{}\u{1e}", fields.join("\u{1f}"));
    }
    ExitCode::SUCCESS
}
