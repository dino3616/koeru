//! `cargo xtask next-id <接頭辞>`

use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, str_of};

/// その接頭辞で、まだ使われていない番号を出す。
///
/// **記録を足すときは、これで番号を取る。** ディレクトリを見て「たぶん次はこれ」と
/// 決めない。既にあるファイルへ上書きすると、そのファイルが持っていた判断が
/// 消え、それを引用していた記録（`decided_by`）だけが残って別のことを指す。
/// 検査は素通りする——形も参照先も壊れないので。**踏んだ**（`DEC-ALL-007` を
/// SPDX の判断で上書きし、`CMP-081` の dlopen の根拠が消えた）。
pub(crate) fn next_id(entries: &[Entry], prefix: &str, mut rep: Report) -> ExitCode {
    let head = format!("{prefix}-");
    let mut used: Vec<u32> = Vec::new();
    // 桁は既にあるものから決める。 既定を 3 に固定すると、2桁で揃っている接頭辞へ
    // 3桁の番号を出す。索引の並びが崩れ、同じ番号の2つ目に見える。**踏んだ。**
    let mut width = 0;
    // 閉包に借りさせない。 借りたままだと、下で `used` と `width` を読めない。
    let take = |used: &mut Vec<u32>, width: &mut usize, id: &str| {
        let Some(tail) = id.strip_prefix(&head) else {
            return;
        };
        // 桁は既にあるものに合わせる。揃っていないと索引の並びが崩れる。
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            *width = (*width).max(tail.len());
            if let Ok(n) = tail.parse::<u32>() {
                used.push(n);
            }
        }
    };
    for e in entries {
        // 1件1ファイルのものと、配列で複数件のもの、両方を見る。
        // 片方だけ見ると、要件のように束で持つ ID を「まだ1件も無い」と言う。
        if let Some(id) = str_of(&e.table, "id") {
            take(&mut used, &mut width, id);
        }
        if let Some((key, _, _)) = e.shape.collection {
            for item in e
                .table
                .get(key)
                .and_then(toml::Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                if let Some(id) = item.as_table().and_then(|t| str_of(t, "id")) {
                    take(&mut used, &mut width, id);
                }
            }
        }
    }

    if used.is_empty() {
        rep.note(format!("`{prefix}` はまだ1件も無い"));
        // 1件も無いなら合わせる先が無い。3桁から始める。
        println!("{head}{:03}", 1);
        return rep.finish("next-id");
    }

    used.sort_unstable();
    let max = used.last().copied().unwrap_or(0);
    // 抜けがあれば言う。埋めるかどうかは人が決めるが、黙って飛ばさせない。
    let missing: Vec<String> = (1..max)
        .filter(|n| !used.contains(n))
        .map(|n| format!("{head}{n:0width$}"))
        .collect();
    if !missing.is_empty() {
        rep.note(format!("抜けている番号がある: {}", missing.join(", ")));
    }
    rep.note(format!(
        "`{prefix}` は {} 件、最大は {head}{max:0width$}",
        used.len()
    ));
    println!("{head}{:0width$}", max + 1);
    rep.finish("next-id")
}
