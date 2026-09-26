//! 本文から ID と引用を拾う。

/// 引用として突き合わせる形は `ID の「…」` だけ。
///
/// 日本語の「」は引用にも強調にも使う。 どちらも検査すると、例示のつもりの
/// 「オフセットだけ直した」まで「原文に無い」と言われる。 そこで
/// 「ID の」を前に置いたときだけ逐語引用とみなす、と決めてある。
/// 逐語で引けないものは `（`TR-SYN-01`。…）` の形で言い換える。
pub(crate) fn citations(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (id, _, end) in id_spans(line) {
        // ID の直後の `` ` `` と空白を飛ばして、「の「」が続くかを見る。
        let rest = line[end..].trim_start_matches(['`', ' ', '\u{3000}']);
        let Some(rest) = rest.strip_prefix("の") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('「') else {
            continue;
        };
        let Some(close) = rest.find('」') else {
            continue;
        };
        out.push((id, rest[..close].to_owned()));
    }
    out
}

/// 引用の突き合わせ用に、空白と約物を落とす。
///
/// 原文は改行やカギ括弧を挟んで書かれていることがあり、
/// そのまま比べると「一字違う」だけで落ちる。 意味を変えない字だけ落とす。
pub(crate) fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '、' | '。' | '，' | '．' | ',' | '.' | '「' | '」' | '`' | '\'' | '"'
                )
        })
        .collect()
}

/// 行から `DEC-REC-007` の形の ID を拾う。
///
/// 前後が英数字・ハイフンでない位置だけを採る。`REQ-REC-005` のような
/// 別の名前空間も同じ形なので、解決先は呼び側が持つ集合が決める。
pub(crate) fn id_tokens(line: &str) -> Vec<String> {
    id_spans(line).into_iter().map(|(id, _, _)| id).collect()
}

/// [`id_tokens`] と同じものを、行内の位置つきで返す。
///
/// 位置が要るのは引用の検査だけ。`ID の「…」` の形かどうかは、
/// ID がどこで終わるかを知らないと判定できない。
fn id_spans(line: &str) -> Vec<(String, usize, usize)> {
    // FSL と meta が使う名前空間を全部挙げる。
    //
    // 挙げ漏らすと、その名前空間の打ち間違いが誰にも見えない
    // ——存在しない `AC-*` を書いても、`AC` を知らなければ ID として拾わない。
    const PREFIX: &[&str] = &[
        "DEC", "TR", "Q", "EVID", "REQ", "PROFILE", "BUDGET", "SCALE", "INV", "CMP", "AC", "FB",
        "TGT", "MODEL",
    ];
    let b = line.as_bytes();
    let mut out = Vec::new();
    for (i, _) in line.char_indices() {
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'-') {
            continue;
        }
        let rest = &line[i..];
        let Some(p) = PREFIX
            .iter()
            .find(|p| rest.starts_with(**p) && rest.as_bytes().get(p.len()) == Some(&b'-'))
        else {
            continue;
        };
        // <PREFIX>-<英大文字>-<数字>
        let after = &rest[p.len() + 1..];
        let area: String = after.chars().take_while(char::is_ascii_uppercase).collect();
        if area.is_empty() {
            continue;
        }
        let tail = &after[area.len()..];
        /*
         * 番号の前のハイフンは、名前空間によって在ったり無かったりする。
         *
         * `TR-REC-02` には在り、`PROFILE-M1` には無い。無い形まで一律に許すと
         * `TR-REC02` のような打ち間違いが実在する ID に化けるので、
         * 無い形を許すのは実際にそう名乗っている名前空間だけにする。
         */
        let (sep, num) = match tail.strip_prefix('-') {
            Some(rest) => (1, rest),
            None if *p == "PROFILE" => (0, tail),
            None => continue,
        };
        let num: String = num.chars().take_while(char::is_ascii_digit).collect();
        if num.is_empty() {
            continue;
        }
        let end = i + p.len() + 1 + area.len() + sep + num.len();
        // 末尾もハイフンで切らない。
        //
        // 英数字だけを見ていると `TR-REC-02-extra` が `TR-REC-02` として通り、
        // 打ち間違いが実在する ID に化ける。前後で同じ規則にする。
        if b.get(end)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'-')
        {
            continue;
        }
        out.push((line[i..end].to_owned(), i, end));
    }
    out
}

/// 文中に現れる `TR-XXX-NN` を拾う。参照先が実在するかを見るために使う。
pub(crate) fn find_tr(text: &str) -> Vec<String> {
    let b: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= b.len() {
        if b[i] == 'T' && b[i + 1] == 'R' && b[i + 2] == '-' {
            let mut j = i + 3;
            let mut alpha = 0;
            while j < b.len() && b[j].is_ascii_uppercase() {
                j += 1;
                alpha += 1;
            }
            if alpha == 3 && j < b.len() && b[j] == '-' {
                let k = j + 1;
                let mut e = k;
                while e < b.len() && b[e].is_ascii_digit() {
                    e += 1;
                }
                if e > k {
                    out.push(b[i..e].iter().collect());
                    i = e;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 番号の前のハイフンが無い名前空間も拾う。ただし勝手に補わない。
    ///
    /// `PROFILE-M1` には区切りが無い。無い形を一律に許すと `TR-REC02` が
    /// `TR-REC-02` に化けるので、許すのはそう名乗っている名前空間だけ。
    #[test]
    fn 区切りの無い形は名前空間で決まる() {
        assert_eq!(id_tokens("`PROFILE-M2` が塞いでいる"), ["PROFILE-M2"]);
        assert!(id_tokens("TR-REC02 は打ち間違い").is_empty());
        assert_eq!(id_tokens("`TR-REC-02` は実在する"), ["TR-REC-02"]);
    }
}
