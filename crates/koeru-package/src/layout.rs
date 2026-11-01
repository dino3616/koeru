//! 枠（`DEC-RCL-018`）を、配布物のどの区画へ置くか。
//!
//! 区画の名前（フォルダ名と音階サフィックス）は、枠の属性のうち音源の中で値が分かれている軸から
//! 作る。 軸が1つしか値を持たないなら、その軸は名前に出さない——声色が1つしか無い音源の
//! フォルダ名に声色を入れても、受け取る人には何も増えない。
//!
//! 1枠しか無い音源は区画を分けない（`None`）。 いまある単一音階の音源は、枠が音高ごとに1つで、
//! 値が分かれる軸は高さだけなので、これまでと同じ名前（`G3` など）になる。

use koeru_model::alias::Method;
use koeru_model::id::SubbankId;
use koeru_model::subbank::{Cell, Cells};
use koeru_model::tone;

/// 区画の名前に出す方式の綴り。 ASCII のみ——フォルダ名と音階サフィックスになる（`TR-PKG-16`）。
#[must_use]
pub const fn method_tag(m: Method) -> &'static str {
    match m {
        Method::Single => "single",
        Method::Sequential => "seq",
        Method::Cvvc => "cvvc",
    }
}

/// 枠ごとの区画の名前。 区画を分けないなら `None`。
///
/// 返す並びは [`Cells`] の並びではなく、高さの低い順（同じ高さは枠の並び順）。 枠の並びは
/// 録る順の既定（`TR-RCL-28`）で、本人が変えられる。 並べ替えるたびに配布物の
/// エントリ順が動くと、同じ素材の書き出しが別物に見える。
#[must_use]
pub fn labels(cells: &Cells) -> Option<Vec<(SubbankId, String)>> {
    let all = cells.as_slice();
    if all.len() <= 1 {
        return None;
    }
    let mut colors = Vec::new();
    let mut methods = Vec::new();
    let mut tones = Vec::new();
    for c in all {
        if !colors.contains(&c.voice_color) {
            colors.push(c.voice_color.clone());
        }
        if !methods.contains(&c.method) {
            methods.push(c.method);
        }
        if !tones.contains(&c.tone) {
            tones.push(c.tone);
        }
    }

    let mut out: Vec<(usize, &Cell, String)> = all
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut parts: Vec<String> = Vec::new();
            if colors.len() > 1
                && let Some(v) = &c.voice_color
            {
                parts.push(v.as_str().to_owned());
            }
            if tones.len() > 1 {
                parts.push(tone::name(c.tone));
            }
            if methods.len() > 1 {
                parts.push(method_tag(c.method).to_owned());
            }
            // 声色だけが分かれていて、声色なしの枠が混じるとき、名前が空になる。
            if parts.is_empty() {
                parts.push(tone::name(c.tone));
            }
            (i, c, parts.join("_"))
        })
        .collect();
    out.sort_by_key(|(i, c, _)| (c.tone, *i));
    Some(out.into_iter().map(|(_, c, l)| (c.id, l)).collect())
}

/// 区画の名前から、書き出す声色の名前を引く。
///
/// 声色を持たない枠は、区画の名前をそのまま `character.yaml` の `color` に書く。
#[must_use]
pub fn yaml_color(cell: &Cell, label: &str) -> String {
    cell.voice_color
        .as_ref()
        .map_or_else(|| label.to_owned(), |v| v.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_model::subbank::VoiceColor;

    fn cell(id: i32, color: Option<&str>, method: Method, tone: i32) -> Cell {
        Cell {
            id: SubbankId::new(id),
            voice_color: color.and_then(VoiceColor::parse),
            method,
            tone,
        }
    }

    fn names(cells: Vec<Cell>) -> Option<Vec<String>> {
        labels(&Cells::new(cells)).map(|v| v.into_iter().map(|(_, l)| l).collect())
    }

    #[test]
    fn 枠が1つなら区画を分けない() {
        assert_eq!(names(vec![cell(1, None, Method::Single, 57)]), None);
        assert_eq!(names(Vec::new()), None);
    }

    /// これまでの単一方式・多音階の音源が、同じ名前になる。
    #[test]
    fn 高さだけが分かれる音源は音名だけ() {
        let got = names(vec![
            cell(1, None, Method::Cvvc, 55),
            cell(2, None, Method::Cvvc, 62),
            cell(3, None, Method::Cvvc, 69),
        ]);
        assert_eq!(got.unwrap(), ["G3", "D4", "A4"]);
    }

    #[test]
    fn 方式が分かれたら方式が名前に入る() {
        let got = names(vec![
            cell(1, None, Method::Single, 57),
            cell(2, None, Method::Cvvc, 57),
        ])
        .unwrap();
        assert_eq!(got, ["single", "cvvc"], "高さが1つなら音名は出さない");
    }

    #[test]
    fn 声色が分かれたら声色が名前に入る() {
        let got = names(vec![
            cell(1, Some("Normal"), Method::Cvvc, 57),
            cell(2, Some("Growl"), Method::Single, 57),
        ])
        .unwrap();
        assert_eq!(got, ["Normal_A3_cvvc", "Growl_A3_single"]);
    }

    #[test]
    fn 声色が1つなら名前に出さない() {
        let got = names(vec![
            cell(1, Some("Normal"), Method::Cvvc, 55),
            cell(2, Some("Normal"), Method::Cvvc, 62),
        ])
        .unwrap();
        assert_eq!(got, ["G3", "D4"]);
    }

    #[test]
    fn 声色なしが混じっても名前は空にならない() {
        let got = names(vec![
            cell(1, None, Method::Cvvc, 57),
            cell(2, Some("Whisper"), Method::Cvvc, 57),
        ])
        .unwrap();
        assert_eq!(got, ["A3", "Whisper"]);
    }

    #[test]
    fn 並びは高さの低い順で_枠の並びに引きずられない() {
        let got = labels(&Cells::new(vec![
            cell(7, None, Method::Single, 69),
            cell(3, None, Method::Single, 55),
        ]))
        .unwrap();
        assert_eq!(got[0].0, SubbankId::new(3));
        assert_eq!(got[1].0, SubbankId::new(7));
    }
}
