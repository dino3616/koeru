//! 同じ起源の調査を、件数として数えない（提案書 10.2 の二つ目の規則）。
//!
//! 同じ原研究を紹介した記事が10本あっても、独立した Evidence は1つ。 件数を確信度に
//! 変えない。 同じ `origin_group`、または同じ派生元（`derived_from`）を1つでも共有する調査は、
//! 同じ起源とみなす。 共有は推移する: A と B が同じ起源で、B と C が同じ起源なら、3つとも
//! 同じ起源。 どちらの欄も持たない調査は、それ自身が1つの起源。

use std::collections::BTreeMap;

use super::facts::Observation;

/// 独立した起源の数。 同じ起源の調査は1つと数える。
pub(crate) fn independent_origins(evidence: &[&Observation]) -> usize {
    origin_groups(evidence).len()
}

/// 起源ごとの調査の ID。 グループは、含む調査の最初の出現順。
pub(crate) fn origin_groups(evidence: &[&Observation]) -> Vec<Vec<String>> {
    let mut parent: Vec<usize> = (0..evidence.len()).collect();
    // 欄の値（束の名前、派生元）から、最初にそれを持った調査へ繋ぐ。
    let mut first_with: BTreeMap<(bool, &str), usize> = BTreeMap::new();
    for (i, e) in evidence.iter().enumerate() {
        let keys = e
            .origin_group
            .iter()
            .map(|g| (false, g.as_str()))
            .chain(e.derived_from.iter().map(|d| (true, d.as_str())));
        for key in keys {
            match first_with.get(&key) {
                Some(&j) => union(&mut parent, i, j),
                None => {
                    first_with.insert(key, i);
                }
            }
        }
    }
    let mut groups: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for (i, e) in evidence.iter().enumerate() {
        let root = find(&mut parent, i);
        groups.entry(root).or_default().push(e.id.clone());
    }
    let mut out: Vec<(usize, Vec<String>)> = groups.into_iter().collect();
    // 根は最初の出現とは限らない。 含む調査のうち一番早いものの位置で並べる。
    out.sort_by_key(|(_, ids)| {
        ids.iter()
            .filter_map(|id| evidence.iter().position(|e| &e.id == id))
            .min()
    });
    out.into_iter().map(|(_, ids)| ids).collect()
}

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn union(parent: &mut [usize], a: usize, b: usize) {
    let (ra, rb) = (find(parent, a), find(parent, b));
    if ra != rb {
        parent[ra.max(rb)] = ra.min(rb);
    }
}
