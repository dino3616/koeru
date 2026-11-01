//! 名前を付けて残す配布物（`DEC-PKG-018`）。 一覧・写す・直す・消す・版を選んで書き出す。
//!
//! 配布物は台帳が持つ（`koeru_core::db::shelf`）。 ここは画面に出す形へ読み替え、入力を確かめ、
//! 書き出しの入口（`packaging::export`）へ配布物と版を渡す。
//!
//! **どれも `open_project` を呼ばない**（`TR-REC-30`）。 開いている収録セッションは続く。
//! 断る理由は値で返す（`DEC-PLT-038`）。 [`AppError`] になるのは台帳やファイルを扱えないときだけ。

use std::collections::BTreeSet;

use koeru_core::db::shelf::{
    DistributionRecord, DistributionRefusal, DistributionRevision, PackagedSubbankRecord,
};
use koeru_core::names;
use koeru_core::release::Release;
use koeru_model::distribution::uncovered;
use koeru_model::id::{DistributionId, SubbankId};
use koeru_model::subbank::{ToneRange, VoiceColor};

use super::subbank::range_violation;
use super::{Studio, SubbankSummary, Violation, now_rfc3339};
use crate::error::{AppError, Result};
use crate::packaging;

/// 配布物が入れる枠1つ（SDL の `PackagedSubbank`）。
#[derive(Debug, Clone, PartialEq)]
pub struct PackagedSubbankSummary {
    /// 枠そのもの。 `range` は枠の担当音域で、この配布物で直した範囲ではない。
    pub subbank: SubbankSummary,
    /// この配布物で書く担当音域（`TR-PKG-59`）。
    pub range: ToneRange,
    /// この配布物で直した範囲か。
    pub range_edited: bool,
}

/// 配布物1つ（SDL の `Distribution`）。
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionSummary {
    pub id: DistributionId,
    pub name: String,
    pub archive_name: String,
    /// 枠の並びの順。
    pub subbanks: Vec<PackagedSubbankSummary>,
    /// 声色ごとの、入れた枠のどれも受け持たない高さ（`TR-PKG-59`）。
    pub uncovered: Vec<(Option<VoiceColor>, ToneRange)>,
    pub terms: Option<String>,
    pub draft_version: Option<String>,
    /// この配布物の書き出し。 古い順。
    pub releases: Vec<Release>,
    /// 最後に書き出したあとに採用が変わった行の数（`TR-PKG-61`）。 書き出していなければ `None`。
    pub rows_changed_since_release: Option<usize>,
}

/// 配布物の操作を断った理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DistributionDenied {
    /// 台帳が断った。
    Refused(DistributionRefusal),
    /// 入力が通らない。 直さずに送り直しても同じになる。
    Invalid(Violation),
}

impl From<DistributionRefusal> for DistributionDenied {
    fn from(r: DistributionRefusal) -> Self {
        Self::Refused(r)
    }
}

/// 配布物の操作の答え。 外側の `Result` は台帳やファイルを扱えなかったこと。
pub type DistributionAnswer<T> = std::result::Result<T, DistributionDenied>;

/// 配布物の枠1つの指定（SDL の `PackagedSubbankInput`）。
#[derive(Debug, Clone, Copy)]
pub struct PackagedSubbankChange {
    pub subbank: SubbankId,
    /// `(下端, 上端)` の MIDI。 省くと枠の担当音域。
    pub range: Option<(i32, i32)>,
}

/// 直す欄（SDL の `ReviseDistributionInput`）。 `None` の欄は変えない。
#[derive(Debug, Clone, Default)]
pub struct ReviseDistribution<'a> {
    pub name: Option<&'a str>,
    pub archive_name: Option<&'a str>,
    /// 渡すと入れる枠をこれに置き換える。 空にはできない。
    pub subbanks: Option<&'a [PackagedSubbankChange]>,
    pub terms: Option<&'a str>,
    pub clear_terms: bool,
    pub draft_version: Option<&'a str>,
    pub clear_draft_version: bool,
}

fn invalid(field: &'static str, code: &'static str) -> DistributionDenied {
    DistributionDenied::Invalid(Violation { field, code })
}

/// 外から入る文字列は境界で NFC へ揃える（`TR-PKG-11`）。
fn nfc(s: &str) -> String {
    koeru_core::text::to_nfc(s)
}

impl Studio {
    /// 配布物を足した順で（SDL の `DistributionFacet.distributions`、`TR-PKG-58`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn distributions(&mut self) -> Result<Vec<DistributionSummary>> {
        let manifest = self.opened()?.dir().read_manifest()?;
        let fallback = names::default_distribution_name(&manifest.display_name);
        let subbanks = self.subbanks()?;
        let mut ledger = self.writer()?;
        let records = ledger.distributions()?;
        let releases = ledger.releases()?;
        let mut out = Vec::with_capacity(records.len());
        for r in records {
            let changed = ledger.rows_changed_since_release(r.id)?;
            let mine = releases
                .iter()
                .filter(|rel| rel.distribution == r.id)
                .cloned()
                .collect();
            out.push(summary(&r, &subbanks, &fallback, mine, changed));
        }
        Ok(out)
    }

    /// 配布物を写して足す（SDL の `duplicateDistribution`、`TR-PKG-58`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self))]
    pub fn duplicate_distribution(
        &mut self,
        source: DistributionId,
    ) -> Result<DistributionAnswer<DistributionSummary>> {
        let added = match self.writer()?.duplicate_distribution(source)? {
            Ok(id) => id,
            Err(r) => return Ok(Err(r.into())),
        };
        self.distribution_summary(added)
    }

    /// 配布物の欄を直す（SDL の `reviseDistribution`、`TR-PKG-58`〜`TR-PKG-60`）。
    ///
    /// 担当音域を直しても、枠の担当音域は変わらない（`TR-PKG-59`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, input))]
    pub fn revise_distribution(
        &mut self,
        id: DistributionId,
        input: &ReviseDistribution<'_>,
    ) -> Result<DistributionAnswer<DistributionSummary>> {
        let name = input.name.map(nfc);
        let archive_name = input.archive_name.map(nfc);
        let terms = input.terms.map(nfc);
        let draft_version = input.draft_version.map(nfc);

        if let Some(n) = &name
            && (n.trim().is_empty() || n.len() > koeru_model::names::MAX_NAME_BYTES)
        {
            return Ok(Err(invalid("name", "distribution.bad_name")));
        }
        if let Some(a) = &archive_name
            && let Some(p) = names::check_segment(a).first()
        {
            return Ok(Err(invalid("archiveName", p.kind())));
        }
        if let Some(v) = &draft_version
            && (v.trim().is_empty() || v.len() > koeru_model::names::MAX_NAME_BYTES)
        {
            return Ok(Err(invalid("draftVersion", "distribution.bad_version")));
        }
        if input.terms.is_some() && input.clear_terms {
            return Ok(Err(invalid("clearTerms", "distribution.terms_and_clear")));
        }
        if input.draft_version.is_some() && input.clear_draft_version {
            return Ok(Err(invalid(
                "clearDraftVersion",
                "distribution.version_and_clear",
            )));
        }
        let members = match input.subbanks.map(members_of).transpose() {
            Ok(m) => m,
            Err(d) => return Ok(Err(d)),
        };

        let change = DistributionRevision {
            name: name.as_deref(),
            archive_name: archive_name.as_deref(),
            subbanks: members.as_deref(),
            terms: terms.as_deref(),
            clear_terms: input.clear_terms,
            draft_version: draft_version.as_deref(),
            clear_draft_version: input.clear_draft_version,
        };
        if let Err(r) = self.writer()?.revise_distribution(id, &change)? {
            return Ok(Err(r.into()));
        }
        self.distribution_summary(id)
    }

    /// 配布物を消す（SDL の `removeDistribution`、`TR-PKG-58`）。 書き出しの記録と ZIP は残る
    /// （`TR-PKG-44`）。 最後の1つは消せない。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self))]
    pub fn remove_distribution(&mut self, id: DistributionId) -> Result<DistributionAnswer<()>> {
        Ok(self
            .writer()?
            .remove_distribution(id)?
            .map_err(DistributionDenied::from))
    }

    /// 配布物と版を選んで書き出す（SDL の `prepareExport` → `commitExport`、`TR-PKG-58`、
    /// `TR-PKG-60`）。
    ///
    /// `distribution` を省くと先頭の配布物。 `version` を省くと下書きの番号で、下書きが無ければ
    /// 最後の版の次。 **書き出し済みの番号を選ぶと、同じ番号で記録を1件足す**——前の ZIP は
    /// 消さない（`DEC-PKG-018`、`TR-PKG-44`）。 `method` が下位の方式なら独立した音源として出す
    /// （`TR-PKG-24`）。
    ///
    /// 指した配布物が無いときは何も書かない。
    ///
    /// # Errors
    ///
    /// 確認が残っている、名前が使えない、被覆が満ちていない、検証に通らない、包めない。
    #[tracing::instrument(skip(self, version))]
    pub fn export_distribution(
        &mut self,
        distribution: Option<DistributionId>,
        version: Option<&str>,
        method: Option<koeru_core::project::Method>,
    ) -> Result<DistributionAnswer<packaging::Exported>> {
        let version = version.map(nfc);
        if version
            .as_deref()
            .is_some_and(|v| v.trim().is_empty() || v.len() > koeru_model::names::MAX_NAME_BYTES)
        {
            return Ok(Err(invalid("version", "distribution.bad_version")));
        }
        let manifest = self.opened()?.dir().read_manifest()?;
        let Some(target) = packaging::target_of(
            &mut *self.writer()?,
            &manifest,
            distribution,
            version.as_deref(),
        )?
        else {
            return Ok(Err(DistributionRefusal::UnknownDistribution.into()));
        };
        let exported = match method {
            Some(m) if m != manifest.method => self.export_downgrade_to(m, &target)?,
            _ => self.export_package_to(&target)?,
        };
        Ok(Ok(exported))
    }

    /// 1つの配布物を、一覧と同じ形で。 足した・直した直後に返す。
    fn distribution_summary(
        &mut self,
        id: DistributionId,
    ) -> Result<DistributionAnswer<DistributionSummary>> {
        Ok(self
            .distributions()?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| DistributionRefusal::UnknownDistribution.into()))
    }

    /// 先頭の配布物と、その下書きの番号（`TR-PKG-60`）。 画面が配布物を意識しない旧い口が使う。
    pub(super) fn first_target(&mut self) -> Result<packaging::Target> {
        let manifest = self.opened()?.dir().read_manifest()?;
        packaging::target_of(&mut *self.writer()?, &manifest, None, None)?.ok_or_else(|| {
            AppError::new(
                "package.no_distribution",
                koeru_failure::Class::Corrupt,
                "配布物が1つも無い",
            )
        })
    }

    /// 書き出しの前に通す関門と、書き出しの時刻。 `export_package` と下位方式で共通。
    pub(super) fn export_gate(&mut self) -> Result<String> {
        // 原音設定の確認が残っているうちは出さない（`INV-ALN-003`）。
        self.ensure_otos_ready()?;
        let pre = self.preflight()?;
        if !pre.may_export() {
            return Err(super::non_nfc_names());
        }
        Ok(now_rfc3339())
    }

    pub(super) fn export_package_to(&mut self, target: &packaging::Target) -> Result<packaging::Exported> {
        let at = self.export_gate()?;
        let dir = self.opened()?.dir().clone();
        let manifest = dir.read_manifest()?;
        let rules = self.current_rules()?;
        packaging::export(&dir, &mut *self.writer()?, &rules, &manifest, &at, target)
    }

    pub(super) fn export_downgrade_to(
        &mut self,
        method: koeru_core::project::Method,
        target: &packaging::Target,
    ) -> Result<packaging::Exported> {
        let at = self.export_gate()?;
        let dir = self.opened()?.dir().clone();
        let manifest = dir.read_manifest()?;
        let rules = self.current_rules()?;
        packaging::export_downgrade(
            &dir,
            &mut *self.writer()?,
            &rules,
            &manifest,
            method,
            &at,
            target,
        )
    }
}

/// 入れる枠の指定を、台帳へ渡す形へ。 空・重複・範囲の誤りは通さない。
fn members_of(
    changes: &[PackagedSubbankChange],
) -> DistributionAnswer<Vec<PackagedSubbankRecord>> {
    if changes.is_empty() {
        return Err(invalid("subbanks", "distribution.no_subbanks"));
    }
    let mut seen = BTreeSet::new();
    let mut out = Vec::with_capacity(changes.len());
    for c in changes {
        if !seen.insert(c.subbank) {
            return Err(invalid("subbanks", "distribution.duplicate_subbank"));
        }
        let edited_range = match c.range.map(|(low, high)| ToneRange::edited(low, high)) {
            None => None,
            Some(Ok(r)) => Some(r),
            Some(Err(e)) => {
                return Err(DistributionDenied::Invalid(Violation {
                    field: "subbanks.toneRange",
                    code: range_violation(e).code,
                }));
            }
        };
        out.push(PackagedSubbankRecord {
            subbank: c.subbank,
            edited_range,
        });
    }
    Ok(out)
}

fn summary(
    r: &DistributionRecord,
    all: &[SubbankSummary],
    fallback_archive_name: &str,
    releases: Vec<Release>,
    rows_changed_since_release: Option<usize>,
) -> DistributionSummary {
    // 枠の並びの順（`RecordingFacet.subbanks` と同じ）。 台帳が消えた枠を持ち越すことは無いが、
    // 読めない枠で一覧を落とさない。
    let subbanks: Vec<PackagedSubbankSummary> = all
        .iter()
        .filter_map(|s| {
            let member = r.subbanks.iter().find(|m| m.subbank == s.id)?;
            Some(PackagedSubbankSummary {
                subbank: s.clone(),
                range: member.edited_range.unwrap_or(s.range),
                range_edited: member.edited_range.is_some(),
            })
        })
        .collect();
    let gaps = uncovered(
        &subbanks
            .iter()
            .map(|m| (m.subbank.voice_color.as_ref(), m.range))
            .collect::<Vec<_>>(),
    );
    DistributionSummary {
        id: r.id,
        name: r.name.clone(),
        archive_name: r
            .archive_name
            .clone()
            .unwrap_or_else(|| fallback_archive_name.to_owned()),
        subbanks,
        uncovered: gaps,
        terms: r.terms.clone(),
        draft_version: r.draft_version.clone(),
        releases,
        rows_changed_since_release,
    }
}