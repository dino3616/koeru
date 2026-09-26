//! プロジェクトのディレクトリ（`TR-PKG-37`, `TR-PKG-38`, `TR-PLT-23`, `TR-PKG-40`）。
//!
//! oto.ini を作業ファイルとして置かない（`TR-PKG-40`）。
//! タイミング5値とエイリアスは DB を正とし、oto.ini は書き出し時に作る派生物。
//! ここに置くと、DB と食い違ったまま外部ツールに編集される。
//!
//! プロジェクトはディレクトリで、名前は不変の UUID。 表示名を変えても
//! ディレクトリ名は動かない。表示名に FS 上不正な文字や CP932 で表現できない
//! 文字が入っても、保存と再開は壊れない（`TR-PKG-37`）。
//!
//! 副作用として、ライブラリ配下を人が見ても中身が判別できない。
//! だから各プロジェクトの直下に人間可読な manifest を平文で置く。
//! これは飾りではなく、UUID 名の代償として要件が課している埋め合わせ。
//!
//! ```text
//! <library>/
//!   0193f0c4-.../          ← 不変の UUID
//!     manifest.toml        ← 人間可読。表示名・方式・項目数
//!     project.db           ← 構造化データ（crate::db）
//!     audio/               ← 録音 WAV。不変資産（`TR-PKG-39`）
//!     renders/             ← 試唱キャッシュ。捨ててよい
//!     exports/             ← 生成済みパッケージ
//!     snapshots/           ← 破壊的操作の直前の DB と manifest（`TR-PKG-43`）
//! ```

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, value};
use uuid::Uuid;

/// manifest の書式版。**読めない版を黙って読まない。**
pub const MANIFEST_VERSION: i64 = 1;

/// 1プロジェクトに残すスナップショットの数（`DEC-PKG-017`）。 超えたぶんは古い順に消す。
///
/// **仮の数。** 録り終えたプロジェクトの台帳の大きさを測っていない。見直す条件もそこにある。
pub const SNAPSHOTS_KEPT: usize = 20;

/// 書きかけに付ける印。 この名前で終わるものは控えでもプロジェクトでもなく、資産でもない。
const STAGING_SUFFIX: &str = ".part";

/// 消しかけの控えに付ける印。 中身を消している途中で落ちても、欠けた控えが控えの名前で残らない。
const DISCARDING_SUFFIX: &str = ".trash";

/// プロジェクトのディレクトリを扱うときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("入出力に失敗した")]
    Io(#[from] std::io::Error),

    /// manifest が TOML として読めない。
    #[error("manifest を解析できない")]
    ManifestSyntax(#[source] toml_edit::TomlError),

    /// manifest に要る鍵が無い、または型が違う。`field` は鍵の名前。
    #[error("manifest の欄が読めない")]
    ManifestField { field: &'static str },

    /// 知らない版の manifest。推測で読まない。
    #[error("このビルドでは扱えない版の manifest")]
    ManifestVersion { found: i64 },

    /// ディレクトリ名が UUID でない。
    #[error("プロジェクトのディレクトリ名が UUID でない")]
    NotAProjectDir,

    /// 知らない方式名。
    ///
    /// **名前を文言に入れない。** manifest は利用者が手で書き換えられるファイルで、
    /// 入っていた文字列がそのまま画面とトレースに出ていた。
    #[error("manifest の方式を知らない")]
    UnknownMethod { found: String },

    /// 台帳の一貫した写しを作れなかった（[`crate::db::write_consistent_copy`]）。
    #[error("台帳を写せなかった")]
    DbCopy(#[source] crate::db::LedgerError),

    /// 控えの印が `[a-z0-9_]+` でない。 印は控えのディレクトリ名に入り、残す数を
    /// 数えるときに名前から連番を読み戻す。
    #[error("控えの印に使えない文字がある")]
    SnapshotLabel,

    /// 控えの連番が、取ってある最新の控えの連番より大きくない。
    ///
    /// 古い順に消すので、小さい連番で取ると、取ったばかりの控えが最も古いものとして消える。
    #[error("控えの連番が最新の控えより古い")]
    SnapshotOutOfOrder,
}

impl koeru_failure::Failure for ProjectError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "project.io",
            Self::ManifestSyntax(_) => "project.manifest_syntax",
            Self::ManifestField { .. } => "project.manifest_field",
            Self::ManifestVersion { .. } => "project.manifest_version",
            Self::NotAProjectDir => "project.not_a_project_dir",
            Self::UnknownMethod { .. } => "project.unknown_method",
            Self::DbCopy(_) => "project.db_copy",
            Self::SnapshotLabel => "project.snapshot_label",
            Self::SnapshotOutOfOrder => "project.snapshot_out_of_order",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        use koeru_failure::Class;
        match self {
            Self::Io(e) => koeru_failure::io_class(e),
            // 新しいビルドが書いたものは壊れていない。このビルドが読めないだけ。
            Self::ManifestVersion { found } if *found > MANIFEST_VERSION => Class::Unsupported,
            Self::ManifestSyntax(_)
            | Self::ManifestField { .. }
            | Self::ManifestVersion { .. }
            | Self::NotAProjectDir
            | Self::UnknownMethod { .. } => Class::Corrupt,
            Self::DbCopy(e) => koeru_failure::Failure::class(e),
            Self::SnapshotLabel => Class::InvalidInput,
            // 最新の控えを読み直してから連番を振り直す。
            Self::SnapshotOutOfOrder => Class::Conflict,
        }
    }
}

type Result<T> = std::result::Result<T, ProjectError>;

/// 収録方式。
///
/// M2 で生成できるのは `Single` だけ（[`crate::reclist::generate_single`]）。
/// 残りは manifest に書けるが、リスト生成はまだ無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Method {
    /// 単独音。
    Single,
    /// 連続音。
    Sequential,
    /// CVVC。
    Cvvc,
    /// 多音階連続音。
    MultiPitchSequential,
}

impl Method {
    /// manifest に書く名前。表示用の日本語ではなく、機械が読む安定した識別子。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Sequential => "sequential",
            Self::Cvvc => "cvvc",
            Self::MultiPitchSequential => "multi_pitch_sequential",
        }
    }

    /// manifest の名前から戻す。
    ///
    /// 画面から作り方を受け取る口（下位方式の書き出し、`TR-PKG-24`）が要る。
    ///
    /// # Errors
    ///
    /// 4つのどれでもない名前。
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "single" => Ok(Self::Single),
            "sequential" => Ok(Self::Sequential),
            "cvvc" => Ok(Self::Cvvc),
            "multi_pitch_sequential" => Ok(Self::MultiPitchSequential),
            other => Err(ProjectError::UnknownMethod {
                found: other.to_owned(),
            }),
        }
    }
}

/// 手渡しの状態（`TR-PKG-33`）。完成判定はこれを参照しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffState {
    /// 一度も書き出していない。
    NotExported,
    /// 書き出したことがある。
    Exported,
}

impl HandoffState {
    /// 画面と IPC へ渡す識別子。
    ///
    /// `Debug` を wire 形式にしない。 `#[derive(Debug)]` の出力は
    /// variant 名の改名で黙って変わるので、TypeScript 側のリテラル union が
    /// コンパイルエラーにならないまま外れる。ここを唯一の対応表にする。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotExported => "NotExported",
            Self::Exported => "Exported",
        }
    }
}

/// カバレッジ側の状態（`TR-PKG-33`, `TR-PKG-34`）。
///
/// 完成 = 必須エイリアス表を 100% 被覆し、全 oto が検証を通り、表示名がある状態。
/// 制作者名義・利用規約・アイコンは条件に入らない（`TR-PKG-34`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverageState {
    /// 必須単位に未収録が残っている。
    Incomplete,
    /// 全部録れたが、oto の検証がまだ通っていない。
    AwaitingOto,
    /// 完成。
    Complete,
}

impl CoverageState {
    /// 画面と IPC へ渡す識別子。
    ///
    /// `Debug` を wire 形式にしない。 `#[derive(Debug)]` の出力は
    /// variant 名の改名で黙って変わるので、TypeScript 側のリテラル union が
    /// コンパイルエラーにならないまま外れる。ここを唯一の対応表にする。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incomplete => "Incomplete",
            Self::AwaitingOto => "AwaitingOto",
            Self::Complete => "Complete",
        }
    }
}

/// プロジェクトの状態。2軸は直交する（`TR-PKG-33`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectState {
    /// 録音と原音設定から機械的に決まる。
    pub coverage: CoverageState,
    /// 書き出し履歴の有無から決まる。
    pub handoff: HandoffState,
}

impl ProjectState {
    /// 完成しているか。
    ///
    /// `handoff` を一切見ない（`TR-PKG-33`）。ZIP を1度も作っていなくても
    /// 完成は完成（`TR-PKG-36`）。
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.coverage, CoverageState::Complete)
    }
}

/// 完成状態を決める（`TR-PKG-34`）。
///
/// 引数は3つとも呼び出し側が DB から引く。ここでは合成規則だけを持つ。
#[must_use]
pub fn coverage_state(
    required: &std::collections::BTreeSet<String>,
    covered: &std::collections::BTreeSet<String>,
    all_oto_validated: bool,
    display_name: &str,
) -> CoverageState {
    if !required.is_subset(covered) {
        return CoverageState::Incomplete;
    }
    if all_oto_validated && !display_name.trim().is_empty() {
        CoverageState::Complete
    } else {
        CoverageState::AwaitingOto
    }
}

/// 人間可読な manifest（`TR-PKG-37`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// 表示名。FS 上不正な文字や CP932 外の文字が入ってよい。
    pub display_name: String,
    pub method: Method,
    /// 録音リストの項目数。
    pub item_count: u32,
    /// 複製元（`TR-PKG-46`）。
    ///
    /// 複製は「派生」として親子関係を残す。 同名・同内容の別プロジェクトが
    /// 並ぶと、どちらが後のものか分からなくなる。
    pub derived_from: Option<Uuid>,
    /// 作ったときの方式プリセット（`TR-RCL-01`）。
    ///
    /// 方式名では足りない。 同じ VCV でも音高が1本か3本かで別のプリセットになる。
    ///
    /// 古いプロジェクトは持っていない。 版を上げて読めなくするより、
    /// 無いことを表せるほうがよい。
    pub preset_id: Option<String>,
    /// 作ったときの音素インベントリの版（`TR-RCL-02`）。
    pub inventory_version: Option<u32>,
}

/// manifest の先頭に置く説明。これが「人が見て判別できる」の実体。
const MANIFEST_HEADER: &str = "\
# KOERU のプロジェクト。**このファイルは中身を人が見分けるために置いてある。**
# ディレクトリ名は不変の UUID なので、名前からは何のプロジェクトか分からない。
#
# **正本は project.db。** ここを手で書き換えても、録音や原音設定は変わらない。
";

impl Manifest {
    fn to_toml(&self) -> String {
        let mut doc = DocumentMut::new();
        doc["version"] = value(MANIFEST_VERSION);
        doc["display_name"] = value(self.display_name.as_str());
        doc["method"] = value(self.method.as_str());
        doc["item_count"] = value(i64::from(self.item_count));
        if let Some(parent) = self.derived_from {
            doc["derived_from"] = value(parent.to_string());
        }
        if let Some(id) = &self.preset_id {
            doc["preset_id"] = value(id.as_str());
        }
        if let Some(v) = self.inventory_version {
            doc["inventory_version"] = value(i64::from(v));
        }
        format!("{MANIFEST_HEADER}\n{doc}")
    }

    fn from_toml(text: &str) -> Result<Self> {
        let doc: DocumentMut = text.parse().map_err(ProjectError::ManifestSyntax)?;

        let found = doc
            .get("version")
            .and_then(toml_edit::Item::as_integer)
            .ok_or(ProjectError::ManifestField { field: "version" })?;
        if found != MANIFEST_VERSION {
            return Err(ProjectError::ManifestVersion { found });
        }

        let display_name = doc
            .get("display_name")
            .and_then(toml_edit::Item::as_str)
            .ok_or(ProjectError::ManifestField {
                field: "display_name",
            })?
            .to_owned();
        let method = Method::parse(
            doc.get("method")
                .and_then(toml_edit::Item::as_str)
                .ok_or(ProjectError::ManifestField { field: "method" })?,
        )?;
        let item_count = doc
            .get("item_count")
            .and_then(toml_edit::Item::as_integer)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(ProjectError::ManifestField {
                field: "item_count",
            })?;

        // 読めない親 UUID は「親なし」に倒さず、失敗として返す。
        // 静かに独立プロジェクトになると、派生関係が消えたことに誰も気づかない。
        let derived_from = match doc.get("derived_from") {
            None => None,
            Some(item) => Some(item.as_str().and_then(|s| Uuid::parse_str(s).ok()).ok_or(
                ProjectError::ManifestField {
                    field: "derived_from",
                },
            )?),
        };

        // 無ければ無いまま持つ。 既定を当てると、古いプロジェクトが
        // 作られたときのプリセットを名乗ることになる。
        let preset_id = doc
            .get("preset_id")
            .and_then(toml_edit::Item::as_str)
            .map(str::to_owned);
        let inventory_version = doc
            .get("inventory_version")
            .and_then(toml_edit::Item::as_integer)
            .and_then(|n| u32::try_from(n).ok());

        Ok(Self {
            display_name,
            method,
            item_count,
            preset_id,
            inventory_version,
            derived_from,
        })
    }
}

/// プロジェクトのディレクトリ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDir {
    id: Uuid,
    root: PathBuf,
}

impl ProjectDir {
    /// 不変の識別子。
    #[must_use]
    pub const fn id(&self) -> Uuid {
        self.id
    }

    /// ディレクトリの根。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 人間可読な manifest。
    #[must_use]
    pub fn manifest_path(&self) -> PathBuf {
        self.root.join("manifest.toml")
    }

    /// この音源の綴りの表（`TR-SYN-36`, `DEC-SYN-010`）。 書き出す `presamp.ini`
    /// （`TR-RCL-24`）と同じ形式。
    ///
    /// **正本ではない。** 正本は台帳の写しで（`DEC-SYN-013`）、ここは本人が
    /// 中身を見られるように置く控え。解決はここを読まない。
    #[must_use]
    pub fn presamp_path(&self) -> PathBuf {
        self.root.join("presamp.ini")
    }

    /// 綴りの表を音源フォルダへ書く（`DEC-SYN-013`）。 作るときに1度だけ。
    ///
    /// # Errors
    ///
    /// 書けないとき。
    #[tracing::instrument(skip(self, text))]
    pub fn write_presamp(&self, text: &str) -> Result<()> {
        write_atomically(&self.presamp_path(), text.as_bytes())
    }

    /// 音源フォルダの `presamp.ini` を、台帳の写しへ揃える（`DEC-SYN-013`）。
    ///
    /// 写しと違えば、書き換えられた中身を `presamp-{SHA-256 の先頭8桁}.ini` として
    /// 同じフォルダに残し、`presamp.ini` を写しの中身へ戻す。返るのは残した
    /// ファイルの名前。**本人が書いたものなので消さない。** 名前に中身の指紋を
    /// 使うので、同じものを何度置いても1つにしかならない。
    ///
    /// 無ければ黙って戻す。 残す中身が無い。
    ///
    /// 突き合わせるのは読んだ文字列。 UTF-8 でも Shift_JIS でも、中身が同じなら
    /// 書き換えられたとみなさない。
    ///
    /// # Errors
    ///
    /// 読めない（無いのは除く）、書けないとき。
    #[tracing::instrument(skip(self, snapshot))]
    pub fn restore_presamp(&self, snapshot: &str) -> Result<Option<String>> {
        use sha2::{Digest as _, Sha256};

        let path = self.presamp_path();
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                write_atomically(&path, snapshot.as_bytes())?;
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        };
        let text = crate::text::decode(&bytes, crate::text::TextEncoding::Utf8)
            .or_else(|_| crate::text::decode(&bytes, crate::text::TextEncoding::Cp932))
            .ok();
        if text.as_deref() == Some(snapshot) {
            return Ok(None);
        }
        let digest = Sha256::digest(&bytes);
        let name = format!(
            "presamp-{}.ini",
            digest[..4]
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        let kept = self.root.join(&name);
        if !kept.exists() {
            write_atomically(&kept, &bytes)?;
        }
        write_atomically(&path, snapshot.as_bytes())?;
        Ok(Some(name))
    }

    /// 構造化データ。
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.root.join("project.db")
    }

    /// 録音 WAV。不変資産（`TR-PKG-39`）。
    #[must_use]
    pub fn audio_dir(&self) -> PathBuf {
        self.root.join("audio")
    }

    /// 試唱キャッシュ。消しても再生成できる。
    #[must_use]
    pub fn renders_dir(&self) -> PathBuf {
        self.root.join("renders")
    }

    /// 生成済みパッケージ。過去のものを上書きしない（`TR-PKG-44`）。
    #[must_use]
    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    /// 破壊的操作の直前の控え（`TR-PKG-43`）。
    #[must_use]
    pub fn snapshots_dir(&self) -> PathBuf {
        self.root.join("snapshots")
    }

    /// manifest を読む。
    #[tracing::instrument(skip(self))]
    pub fn read_manifest(&self) -> Result<Manifest> {
        let mut m = Manifest::from_toml(&fs::read_to_string(self.manifest_path())?)?;
        // ファイル読み込みも外から文字列が入る境界（`TR-PKG-11`）。
        //
        // **書くときに揃えるだけでは足りない。** 分解形のまま書かれた
        // manifest が既にあると、そこから作った配布物の `character.txt` も
        // `readme.txt` も分解形のまま出る。
        m.display_name = crate::text::to_nfc(&m.display_name);
        Ok(m)
    }

    /// manifest を書く。一時ファイル → fsync → rename（`TR-PKG-41`）。
    ///
    /// 途中で落ちても、部分的に書かれた manifest は残らない。
    #[tracing::instrument(skip(self, m))]
    pub fn write_manifest(&self, m: &Manifest) -> Result<()> {
        write_atomically(&self.manifest_path(), m.to_toml().as_bytes())
    }

    /// 破壊的操作の直前に控えを取る（`TR-PKG-43`）。
    ///
    /// WAV は複製しない。 DB と manifest だけを複製し、WAV は元を参照する。
    /// 3時間の録音を操作のたびに二重化したら、ディスクがいくつあっても足りない。
    ///
    /// `seq` は呼び出し側が単調増加で与える。`label` は操作の名前
    /// （`realign` / `downgrade_export` / `bulk_alias` / `delete_items` / `change_method`）。
    ///
    /// 控えは `snapshots/{seq:06}-{label}.part/` で組み立て、中身と入れ物を fsync してから
    /// 名前を付け替えて出す。 途中で落ちても、欠けた控えが控えの名前を名乗らない。
    /// 出し終えてから、[`SNAPSHOTS_KEPT`] を超えた古い控えを消す。
    ///
    /// # Errors
    ///
    /// `label` が印に使えない（[`ProjectError::SnapshotLabel`]）、`seq` が最新の控え以下
    /// （[`ProjectError::SnapshotOutOfOrder`]）、台帳を写せない（[`ProjectError::DbCopy`]）、
    /// manifest を読めない・書けない。 どれで落ちても、今ある控えは消さない。
    #[tracing::instrument(skip(self))]
    pub fn take_snapshot(&self, seq: u32, label: &str) -> Result<PathBuf> {
        if !is_snapshot_label(label) {
            return Err(ProjectError::SnapshotLabel);
        }
        let root = self.snapshots_dir();
        fs::create_dir_all(&root)?;
        if self
            .published_snapshots()?
            .last()
            .is_some_and(|(last, _)| *last >= seq)
        {
            return Err(ProjectError::SnapshotOutOfOrder);
        }

        let name = format!("{seq:06}-{label}");
        let dir = root.join(&name);
        let staging = root.join(format!("{name}{STAGING_SUFFIX}"));
        let published = self
            .stage_snapshot(&staging)
            .and_then(|()| fs::rename(&staging, &dir).map_err(ProjectError::from));
        if let Err(e) = published {
            // 書きかけは控えではない。 残すと、同じ連番で取り直したときに塞ぐ。
            let _ = fs::remove_dir_all(&staging);
            return Err(e);
        }
        // ディレクトリエントリを永続化する。中身だけ fsync しても、
        // ディレクトリが飛べば控えは無い。
        fsync_dir(&root)?;

        // 控えは出せている。 消せなかった古い控えは次の控えで消し直すので、
        // 取ったこと自体は失敗にしない。
        if let Err(e) = self.prune_snapshots() {
            koeru_failure::record_failure(&e, koeru_failure::Outcome::Committed, "snapshot_prune");
        }
        Ok(dir)
    }

    /// 書きかけの控えを `staging` に組み立てる。 出すのは呼び出し側。
    fn stage_snapshot(&self, staging: &Path) -> Result<()> {
        // 前に落ちたときの書きかけ。 出していないので控えではない。
        remove_dir_if_present(staging)?;
        fs::create_dir(staging)?;
        crate::db::write_consistent_copy(&self.db_path(), &staging.join("project.db"))
            .map_err(ProjectError::DbCopy)?;
        sync_file(&staging.join("project.db"))?;
        copy_synced(&self.manifest_path(), &staging.join("manifest.toml"))?;
        fsync_dir(staging)
    }

    /// [`SNAPSHOTS_KEPT`] を超えた古い控えを消す。 返るのは消した数。
    ///
    /// 先に名前を付け替えてから中身を消す。 消している途中で落ちても、欠けた控えが
    /// 控えの名前で残らない。付け替えたまま残ったものは、次に呼んだときに消す。
    fn prune_snapshots(&self) -> Result<usize> {
        let root = self.snapshots_dir();
        for entry in fs::read_dir(&root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .ends_with(DISCARDING_SUFFIX)
            {
                fs::remove_dir_all(entry.path())?;
            }
        }

        let published = self.published_snapshots()?;
        let excess = published.len().saturating_sub(SNAPSHOTS_KEPT);
        if excess == 0 {
            return Ok(0);
        }
        let mut discarding = Vec::with_capacity(excess);
        for (_, dir) in &published[..excess] {
            let mut name = dir.clone().into_os_string();
            name.push(DISCARDING_SUFFIX);
            let to = PathBuf::from(name);
            fs::rename(dir, &to)?;
            discarding.push(to);
        }
        fsync_dir(&root)?;
        for dir in &discarding {
            fs::remove_dir_all(dir)?;
        }
        fsync_dir(&root)?;
        tracing::debug!(count = excess, "古い控えを消した");
        Ok(excess)
    }

    /// 取ってある控えを古い順に挙げる。
    ///
    /// 順は名前の連番を数として読んで決める。 辞書順だと、連番が6桁を超えたところで
    /// `1000000-…` が `999999-…` より前に来る。 書きかけ・消しかけと、形の違う名前は挙げない。
    #[tracing::instrument(skip(self))]
    pub fn snapshots(&self) -> Result<Vec<PathBuf>> {
        Ok(self
            .published_snapshots()?
            .into_iter()
            .map(|(_, dir)| dir)
            .collect())
    }

    /// 出し終えた控えと、その連番。 古い順。
    fn published_snapshots(&self) -> Result<Vec<(u32, PathBuf)>> {
        let dir = self.snapshots_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            if let Some(seq) = entry.file_name().to_str().and_then(snapshot_seq) {
                out.push((seq, entry.path()));
            }
        }
        out.sort();
        Ok(out)
    }
}

/// 控えのディレクトリ名 `{seq:06}-{label}` から連番を読む。 その形でなければ `None`。
fn snapshot_seq(name: &str) -> Option<u32> {
    let (seq, label) = name.split_once('-')?;
    if seq.is_empty() || !seq.bytes().all(|b| b.is_ascii_digit()) || !is_snapshot_label(label) {
        return None;
    }
    seq.parse().ok()
}

/// 控えの印に使える形か。 `.` を許すと、書きかけの印と見分けがつかない。
fn is_snapshot_label(label: &str) -> bool {
    !label.is_empty()
        && label
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// アプリが管理するライブラリ（`TR-PKG-37`）。
///
/// 利用者にフォルダ操作を要求しない（`TR-PKG-45`）。保存先の選択も、
/// WAV のリネームも、バックアップフォルダ作りもここが引き受ける。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    /// ライブラリを開く。無ければ作る。
    #[tracing::instrument(skip(root))]
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// ライブラリの根。通常モードの画面には出さない（`TR-PKG-45`）。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// プロジェクトを作る。ディレクトリ名は UUID で、以後変えない。
    #[tracing::instrument(skip(self, m))]
    pub fn create(&self, m: &Manifest) -> Result<ProjectDir> {
        let id = Uuid::new_v4();
        let dir = ProjectDir {
            id,
            root: self.root.join(id.to_string()),
        };
        lay_out(&dir)?;
        dir.write_manifest(m)?;
        fsync_dir(&self.root)?;
        Ok(dir)
    }

    /// プロジェクトを複製して派生を作る（`TR-PKG-46`）。
    ///
    /// 親子関係を manifest に残す。 同じライブラリに同名・同内容のものが
    /// 並んでも、どちらから出たかは辿れる。
    ///
    /// WAV は複製する。元は不変資産なので参照でも足りるが、片方を消したときに
    /// もう片方の音が消えるのは説明がつかない（`TR-PKG-39`）。
    ///
    /// 派生は `{uuid}.part/` で組み立て、写し終えてから UUID の名前へ付け替える。
    /// 途中で落ちても、台帳が指す WAV を欠いたプロジェクトが一覧に出ない。
    #[tracing::instrument(skip(self, parent, display_name))]
    pub fn derive(&self, parent: &ProjectDir, display_name: &str) -> Result<ProjectDir> {
        let mut m = parent.read_manifest()?;
        m.display_name = display_name.to_owned();
        m.derived_from = Some(parent.id());

        let id = Uuid::new_v4();
        let child = ProjectDir {
            id,
            root: self.root.join(id.to_string()),
        };
        let staging = ProjectDir {
            id,
            root: self.root.join(format!("{id}{STAGING_SUFFIX}")),
        };
        let published = stage_derived(parent, &staging, &m)
            .and_then(|()| fs::rename(staging.root(), child.root()).map_err(ProjectError::from));
        if let Err(e) = published {
            let _ = fs::remove_dir_all(staging.root());
            return Err(e);
        }
        fsync_dir(&self.root)?;
        Ok(child)
    }

    /// 既にあるプロジェクトを開く。
    #[tracing::instrument(skip(self))]
    pub fn open_project(&self, id: Uuid) -> Result<ProjectDir> {
        let root = self.root.join(id.to_string());
        if !root.is_dir() {
            return Err(ProjectError::NotAProjectDir);
        }
        Ok(ProjectDir { id, root })
    }

    /// ライブラリの中身を挙げる。
    ///
    /// manifest が読めないものは飛ばさず、失敗として返す。 一覧から静かに
    /// 消えると、利用者は「プロジェクトが無くなった」と受け取る。
    #[tracing::instrument(skip(self))]
    pub fn list(&self) -> Result<Vec<(ProjectDir, std::result::Result<Manifest, ProjectError>)>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(id) = Uuid::parse_str(&name) else {
                continue;
            };
            let dir = ProjectDir {
                id,
                root: entry.path(),
            };
            let m = dir.read_manifest();
            out.push((dir, m));
        }
        out.sort_by_key(|(d, _)| d.id);
        Ok(out)
    }
}

/// プロジェクトのディレクトリ一式を作る（`TR-PKG-38`）。
fn lay_out(dir: &ProjectDir) -> Result<()> {
    for d in [
        dir.root.clone(),
        dir.audio_dir(),
        dir.renders_dir(),
        dir.exports_dir(),
        dir.snapshots_dir(),
    ] {
        fs::create_dir_all(&d)?;
    }
    Ok(())
}

/// 派生を `staging` に組み立てる。 出すのは呼び出し側（[`Library::derive`]）。
fn stage_derived(parent: &ProjectDir, staging: &ProjectDir, m: &Manifest) -> Result<()> {
    lay_out(staging)?;
    staging.write_manifest(m)?;
    // 台帳を先に写す（録音も原音設定も引き継ぐのが「複製」）。 WAV は確定（rename）して
    // から台帳へ載る（`DEC-REC-004`）ので、写した台帳が指す WAV は、このあと写す音声に
    // 必ずある。 逆の順だと、その間に確定したテイクの行だけが写る。
    if parent.db_path().is_file() {
        crate::db::write_consistent_copy(&parent.db_path(), &staging.db_path())
            .map_err(ProjectError::DbCopy)?;
        sync_file(&staging.db_path())?;
    }
    copy_tree(&parent.audio_dir(), &staging.audio_dir())?;
    fsync_dir(staging.root())
}

/// `src` の中身を入れ子ごと `dest` へ写す。 バイトは変えない。
///
/// 多音階の WAV は音高ごとのディレクトリにある（`TR-REC-36`）。 直下だけを写すと、
/// 多音階の派生から音声が全部落ちる（`EVID-PLT-004`）。
///
/// 書きかけ（`.part` で終わるもの）は写さない。 確定していないので資産ではなく、
/// 写した台帳もそれを指さない。 symlink も辿らない。 KOERU は作らない。
fn copy_tree(src: &Path, dest: &Path) -> Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dest.join(entry.file_name());
        if ty.is_dir() {
            fs::create_dir_all(&to)?;
            copy_tree(&entry.path(), &to)?;
        } else if ty.is_file()
            && !entry
                .file_name()
                .to_string_lossy()
                .ends_with(STAGING_SUFFIX)
        {
            copy_synced(&entry.path(), &to)?;
        }
    }
    fsync_dir(dest)
}

/// 写して fsync する。 入れ物のディレクトリは呼び出し側が fsync する。
fn copy_synced(src: &Path, dest: &Path) -> Result<()> {
    fs::copy(src, dest)?;
    sync_file(dest)
}

/// 書き終えたファイルを fsync する。
///
/// 書ける形で開き直す。 Windows の `FlushFileBuffers` は、読むだけの handle では拒まれる。
fn sync_file(path: &Path) -> Result<()> {
    fs::OpenOptions::new().write(true).open(path)?.sync_all()?;
    Ok(())
}

/// ディレクトリがあれば中身ごと消す。 無ければ何もしない。
fn remove_dir_if_present(path: &Path) -> Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

/// 一時ファイルへ書いて fsync し、アトミックな rename で置き換える（`TR-PKG-41`）。
///
/// rename が成功するまで、元のファイルはそのまま。 途中で落ちても、
/// 半分書かれたファイルが正規の名前を名乗ることはない。
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("toml.part");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        fsync_dir(parent)?;
    }
    Ok(())
}

fn fsync_dir(path: &Path) -> Result<()> {
    sync_dir(path)?;
    Ok(())
}

/// ディレクトリエントリを永続化する。
///
/// ファイルを fsync しても、ディレクトリを fsync しないと rename が飛ぶ。
/// Windows にはディレクトリを開く経路が無いので、そこでは何もしない
/// （NTFS のメタデータ更新はジャーナルで守られる）。
///
/// # Errors
///
/// ディレクトリを開けない、fsync が失敗した。
pub fn sync_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        fs::File::open(path)?.sync_all()?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// テスト用の一時ディレクトリ。プロセス ID と連番で衝突を避ける。
    fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("koeru-proj-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
        d
    }

    fn manifest() -> Manifest {
        Manifest {
            display_name: "こえるちゃん".to_owned(),
            method: Method::Single,
            item_count: 102,
            derived_from: None,
            preset_id: None,
            inventory_version: None,
        }
    }

    /// 書き換えられた `presamp.ini` は戻し、中身を別名で残す（`DEC-SYN-013`）。
    #[test]
    fn 書き換えられた表は戻して残す() {
        let lib = Library::open(tmp("presamp")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let snapshot = "[VERSION]\r\n1.0\r\n";

        // 無ければ黙って戻す。
        assert_eq!(p.restore_presamp(snapshot).expect("揃う"), None);
        assert_eq!(
            fs::read_to_string(p.presamp_path()).expect("ある"),
            snapshot
        );

        // 揃っていれば何もしない。
        assert_eq!(p.restore_presamp(snapshot).expect("揃う"), None);

        // 書き換えられていれば、中身を残して戻す。
        fs::write(p.presamp_path(), "[BEGINING_CV]\r\n-%CV%\r\n").expect("書ける");
        let kept = p.restore_presamp(snapshot).expect("揃う").expect("残した");
        assert!(
            kept.starts_with("presamp-") && kept.ends_with(".ini"),
            "{kept}"
        );
        assert_eq!(kept.len(), "presamp-".len() + 8 + ".ini".len());
        assert_eq!(
            fs::read_to_string(p.root().join(&kept)).expect("残っている"),
            "[BEGINING_CV]\r\n-%CV%\r\n"
        );
        assert_eq!(
            fs::read_to_string(p.presamp_path()).expect("ある"),
            snapshot
        );

        // 同じものを置き直しても、残すのは1つ。
        fs::write(p.presamp_path(), "[BEGINING_CV]\r\n-%CV%\r\n").expect("書ける");
        assert_eq!(p.restore_presamp(snapshot).expect("揃う"), Some(kept));
        let kept_files = fs::read_dir(p.root())
            .expect("読める")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .filter(|n| n.starts_with("presamp-"))
            .count();
        assert_eq!(kept_files, 1);
    }

    #[test]
    fn create_lays_out_the_directory() {
        let lib = Library::open(tmp("layout")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");

        assert!(p.manifest_path().is_file());
        assert!(p.audio_dir().is_dir());
        assert!(p.renders_dir().is_dir());
        assert!(p.exports_dir().is_dir());
        assert!(p.snapshots_dir().is_dir());

        // ディレクトリ名は UUID（`TR-PKG-37`）。
        let name = p.root().file_name().and_then(|s| s.to_str()).expect("名前");
        assert_eq!(Uuid::parse_str(name).expect("UUID であること"), p.id());
    }

    #[test]
    fn manifest_round_trips() {
        let lib = Library::open(tmp("round")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        assert_eq!(p.read_manifest().expect("読めること"), manifest());
    }

    /// manifest は人が読めること（`TR-PKG-37`）。
    /// UUID 名の代償を埋め合わせるために置いているので、表示名がそのまま見えないと意味がない。
    #[test]
    fn manifest_is_human_readable() {
        let lib = Library::open(tmp("readable")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let text = fs::read_to_string(p.manifest_path()).expect("読めること");

        assert!(text.contains("こえるちゃん"), "表示名がそのまま見えること");
        assert!(text.contains("single"), "方式が見えること");
        assert!(text.contains("102"), "項目数が見えること");
        assert!(
            text.starts_with('#'),
            "何のファイルかの説明が先頭にあること"
        );
    }

    /// 表示名に FS 上不正な文字や CP932 外の文字が入っても壊れない（`TR-PKG-37`）。
    #[test]
    fn display_name_may_be_hostile() {
        let lib = Library::open(tmp("hostile")).expect("開けること");
        let hostile = Manifest {
            // スラッシュ・コロン・NUL 以外の制御文字・絵文字・CP932 外の漢字。
            display_name: "a/b:c*d?\u{7}🎤𠮷 \"quoted\"".to_owned(),
            method: Method::Sequential,
            item_count: 7,
            derived_from: None,
            preset_id: None,
            inventory_version: None,
        };
        let p = lib.create(&hostile).expect("作れること");
        assert_eq!(p.read_manifest().expect("読めること"), hostile);

        // ディレクトリ名は表示名に一切影響されない。
        let name = p.root().file_name().and_then(|s| s.to_str()).expect("名前");
        assert!(Uuid::parse_str(name).is_ok());
    }

    /// 改名でディレクトリ名を変えない（`TR-PKG-37`）。
    #[test]
    fn rename_does_not_move_the_directory() {
        let lib = Library::open(tmp("rename")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let before = p.root().to_path_buf();

        let mut m = manifest();
        m.display_name = "別の名前".to_owned();
        p.write_manifest(&m).expect("書けること");

        assert_eq!(p.root(), before);
        assert_eq!(
            p.read_manifest().expect("読めること").display_name,
            "別の名前"
        );
    }

    /// 部分的に書かれた manifest を残さない（`TR-PKG-41`）。
    #[test]
    fn manifest_write_leaves_no_partial_file() {
        let lib = Library::open(tmp("atomic")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        p.write_manifest(&manifest()).expect("書けること");

        let leftovers: Vec<_> = fs::read_dir(p.root())
            .expect("読めること")
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".part"))
            .collect();
        assert!(leftovers.is_empty(), "書きかけが残らないこと");
    }

    #[test]
    fn unknown_manifest_version_is_refused() {
        let e = Manifest::from_toml(
            "version = 99\ndisplay_name = 'x'\nmethod = 'single'\nitem_count = 1",
        )
        .expect_err("拒むこと");
        assert!(matches!(e, ProjectError::ManifestVersion { found: 99 }));
        // 新しいビルドが書いたものを、壊れていると見せない。
        assert_eq!(
            koeru_failure::Failure::class(&e),
            koeru_failure::Class::Unsupported
        );
    }

    #[test]
    fn unknown_method_is_refused() {
        let e = Manifest::from_toml(
            "version = 1\ndisplay_name = 'x'\nmethod = 'vcv-ish'\nitem_count = 1",
        )
        .expect_err("拒むこと");
        assert_eq!(koeru_failure::Failure::code(&e), "project.unknown_method");
        assert!(
            !e.to_string().contains("vcv-ish"),
            "manifest の文字列を文言に入れない"
        );
    }

    #[test]
    fn list_returns_projects_in_a_stable_order() {
        let lib = Library::open(tmp("list")).expect("開けること");
        let a = lib.create(&manifest()).expect("作れること");
        let b = lib.create(&manifest()).expect("作れること");
        // UUID とは無関係のディレクトリは無視する。
        fs::create_dir_all(lib.root().join("not-a-project")).expect("作れること");

        let listed = lib.list().expect("挙げられること");
        assert_eq!(listed.len(), 2);
        let mut want = [a.id(), b.id()];
        want.sort();
        assert_eq!([listed[0].0.id(), listed[1].0.id()], want);
    }

    /// 読めない manifest を一覧から静かに消さない。
    #[test]
    fn list_reports_broken_manifests_instead_of_hiding_them() {
        let lib = Library::open(tmp("broken")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        fs::write(p.manifest_path(), "これは TOML ではない = = =").expect("書けること");

        let listed = lib.list().expect("挙げられること");
        assert_eq!(listed.len(), 1, "一覧から消えないこと");
        assert!(listed[0].1.is_err(), "失敗として返ること");
    }

    /// 読むだけで開いて行を数える。 開けない・表が無いなら `None`。
    fn rows_in(db: &Path) -> Option<i64> {
        use diesel::prelude::*;
        let mut c =
            diesel::sqlite::SqliteConnection::establish(&crate::db::read_only_url(db)).ok()?;
        crate::schema::rows::table.count().get_result(&mut c).ok()
    }

    /// 台帳を開いて行を入れる。 返す台帳は開いたままにしておく——最後の接続が
    /// 閉じるとチェックポイントが走り、WAL が本体へ畳まれる。
    fn ledger_with_rows(p: &ProjectDir) -> (crate::db::Ledger, i64) {
        let mut ledger = crate::db::Ledger::open(p.db_path()).expect("開けること");
        let list = crate::reclist::generate_single(crate::inventory::UnitSet::Core, 5)
            .expect("生成できること");
        ledger.install_reclist(&list, 60).expect("書けること");
        let n = rows_in(&p.db_path()).expect("数えられること");
        assert!(n > 0, "行が入ったこと");
        (ledger, n)
    }

    fn names_in(dir: &Path) -> BTreeSet<String> {
        fs::read_dir(dir)
            .expect("読めること")
            .filter_map(|e| e.ok()?.file_name().into_string().ok())
            .collect()
    }

    fn snapshot_names(p: &ProjectDir) -> Vec<String> {
        p.snapshots()
            .expect("挙げられること")
            .iter()
            .filter_map(|d| d.file_name().and_then(|s| s.to_str()).map(str::to_owned))
            .collect()
    }

    /// 控えは WAV を複製しない（`TR-PKG-43`）。
    #[test]
    fn snapshot_copies_the_db_but_not_the_audio() {
        let lib = Library::open(tmp("snap")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&p);
        // 3時間ぶんのつもりの WAV。
        fs::write(p.audio_dir().join("a.wav"), vec![0_u8; 4096]).expect("書けること");

        let dir = p.take_snapshot(1, "realign").expect("取れること");

        assert_eq!(
            names_in(&dir),
            BTreeSet::from(["manifest.toml".to_owned(), "project.db".to_owned()]),
            "台帳と manifest だけ。WAV も `-wal` も書きかけも無いこと"
        );
        assert_eq!(
            fs::read(dir.join("manifest.toml")).expect("読めること"),
            fs::read(p.manifest_path()).expect("読めること")
        );

        // 元の WAV はそのまま残る（参照する側）。
        assert!(p.audio_dir().join("a.wav").is_file());
    }

    /// 控えは WAL に残ったコミット済みの中身ごと写す（`TR-PKG-43`、`EVID-PLT-004`）。
    ///
    /// 台帳の本体を `fs::copy` していた頃は、チェックポイント前のコミットが控えから落ちていた。
    #[test]
    fn snapshot_keeps_commits_still_in_the_wal() {
        let lib = Library::open(tmp("wal")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (ledger, n) = ledger_with_rows(&p);

        // 前提: 行はまだ WAL にしか無い。 本体だけを写すと行が見えない。
        let wal = p.root().join("project.db-wal");
        assert!(
            fs::metadata(&wal).expect("WAL があること").len() > 0,
            "チェックポイント前であること"
        );
        let naive = tmp("wal-naive").join("project.db");
        fs::copy(p.db_path(), &naive).expect("写せること");
        assert_ne!(
            rows_in(&naive),
            Some(n),
            "本体だけの写しは行を落とすこと。落とさないなら、この試験は何も見ていない"
        );

        let dir = p.take_snapshot(1, "realign").expect("取れること");
        assert_eq!(rows_in(&dir.join("project.db")), Some(n));
        // 控えは単体で開ける。 `-wal` を伴わない。
        assert!(!dir.join("project.db-wal").exists());
        drop(ledger);
    }

    /// ライブラリの置き場所は利用者名を含む。 URI で意味を持つ文字が入っても控えを取れる。
    #[test]
    fn snapshot_survives_uri_characters_in_the_path() {
        let lib = Library::open(tmp("uri").join("ライブラリ 100%#1")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, n) = ledger_with_rows(&p);

        let dir = p.take_snapshot(1, "realign").expect("取れること");
        assert_eq!(rows_in(&dir.join("project.db")), Some(n));
    }

    #[test]
    fn snapshots_are_listed_oldest_first() {
        let lib = Library::open(tmp("snaps")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&p);

        for (seq, label) in [(1, "realign"), (2, "bulk_alias"), (10, "change_method")] {
            p.take_snapshot(seq, label).expect("取れること");
        }
        assert_eq!(
            snapshot_names(&p),
            [
                "000001-realign",
                "000002-bulk_alias",
                "000010-change_method"
            ]
        );

        // 6桁を超えても連番の順。 名前の辞書順では `1000000-…` が先に来る。
        p.take_snapshot(999_999, "realign").expect("取れること");
        p.take_snapshot(1_000_000, "realign").expect("取れること");
        let got = snapshot_names(&p);
        assert_eq!(
            got[got.len() - 2..],
            ["999999-realign", "1000000-realign"],
            "連番を数として読んで並べること"
        );
    }

    /// 直近の [`SNAPSHOTS_KEPT`] 件を残し、超えたぶんは古い順に消す。
    #[test]
    fn snapshots_beyond_the_limit_are_removed_oldest_first() {
        let lib = Library::open(tmp("keep")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&p);

        let taken = u32::try_from(SNAPSHOTS_KEPT + 2).expect("小さい");
        for seq in 1..=taken {
            p.take_snapshot(seq, "realign").expect("取れること");
        }

        let want: Vec<String> = (3..=taken).map(|s| format!("{s:06}-realign")).collect();
        assert_eq!(snapshot_names(&p), want, "古い2件が消え、残りは連番の順");
        assert_eq!(
            names_in(&p.snapshots_dir()),
            want.iter().cloned().collect(),
            "消しかけも書きかけも残らないこと"
        );
    }

    /// 途中で落ちても、欠けた控えを出さず、今ある控えを消さない（`TR-PKG-43`）。
    #[test]
    fn failed_snapshot_is_not_published_and_keeps_the_old_ones() {
        let lib = Library::open(tmp("snapfail")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (ledger, _) = ledger_with_rows(&p);

        let kept = u32::try_from(SNAPSHOTS_KEPT).expect("小さい");
        for seq in 1..=kept {
            p.take_snapshot(seq, "realign").expect("取れること");
        }
        let before = names_in(&p.snapshots_dir());
        assert_eq!(before.len(), SNAPSHOTS_KEPT);

        // 台帳を写したあとで落ちる。 manifest が読めない。
        let manifest = fs::read(p.manifest_path()).expect("読めること");
        fs::remove_file(p.manifest_path()).expect("消せること");
        let e = p
            .take_snapshot(kept + 1, "realign")
            .expect_err("落ちること");
        assert!(matches!(e, ProjectError::Io(_)), "{e:?}");
        assert_eq!(
            names_in(&p.snapshots_dir()),
            before,
            "書きかけを残さず、最も古い控えも消さないこと"
        );

        // 台帳を写せないところで落ちる。
        fs::write(p.manifest_path(), &manifest).expect("戻せること");
        drop(ledger);
        fs::write(p.db_path(), "SQLite ではない").expect("書けること");
        let e = p
            .take_snapshot(kept + 1, "realign")
            .expect_err("落ちること");
        assert_eq!(koeru_failure::Failure::code(&e), "project.db_copy");
        assert_eq!(names_in(&p.snapshots_dir()), before);
    }

    /// 前に落ちたときの書きかけは、同じ連番で取り直すときに塞がない。
    #[test]
    fn stale_staging_does_not_block_a_retry() {
        let lib = Library::open(tmp("stale")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, n) = ledger_with_rows(&p);
        let stale = p.snapshots_dir().join("000001-realign.part");
        fs::create_dir_all(&stale).expect("作れること");
        fs::write(stale.join("project.db"), b"half").expect("書けること");

        let dir = p.take_snapshot(1, "realign").expect("取れること");
        assert_eq!(rows_in(&dir.join("project.db")), Some(n));
        assert!(!stale.exists());
        assert_eq!(snapshot_names(&p), ["000001-realign"]);
    }

    /// 古い順に消すので、連番が戻ると取ったばかりの控えが消える。 受けない。
    #[test]
    fn snapshot_sequence_must_move_forward() {
        let lib = Library::open(tmp("seq")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&p);
        p.take_snapshot(5, "realign").expect("取れること");

        for seq in [5, 4] {
            let e = p.take_snapshot(seq, "bulk_alias").expect_err("拒むこと");
            assert!(matches!(e, ProjectError::SnapshotOutOfOrder), "{e:?}");
        }
        assert_eq!(snapshot_names(&p), ["000005-realign"]);
    }

    /// 印は名前に入り、連番を読み戻すときに形を見る。 読み戻せない印では取らない。
    #[test]
    fn snapshot_label_must_be_readable_back() {
        let lib = Library::open(tmp("label")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&p);

        for label in ["", "a.part", "../x", "Realign", "a-b"] {
            let e = p.take_snapshot(1, label).expect_err("拒むこと");
            assert!(matches!(e, ProjectError::SnapshotLabel), "{label}: {e:?}");
        }
        assert!(names_in(&p.snapshots_dir()).is_empty());
    }

    fn units(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn coverage_is_incomplete_while_units_are_missing() {
        let s = coverage_state(&units(&["か", "き"]), &units(&["か"]), true, "名前");
        assert_eq!(s, CoverageState::Incomplete);
    }

    #[test]
    fn coverage_waits_for_oto_and_for_a_display_name() {
        let req = units(&["か"]);
        assert_eq!(
            coverage_state(&req, &req, false, "名前"),
            CoverageState::AwaitingOto
        );
        assert_eq!(
            coverage_state(&req, &req, true, "   "),
            CoverageState::AwaitingOto,
            "表示名は完成の必要条件（TR-PKG-34）"
        );
        assert_eq!(
            coverage_state(&req, &req, true, "名前"),
            CoverageState::Complete
        );
    }

    /// 完成判定は書き出し履歴を見ない（`TR-PKG-33`, `TR-PKG-36`）。
    /// ZIP を1度も作っていなくても完成は完成。
    #[test]
    fn completeness_ignores_handoff() {
        for handoff in [HandoffState::NotExported, HandoffState::Exported] {
            let s = ProjectState {
                coverage: CoverageState::Complete,
                handoff,
            };
            assert!(s.is_complete());
        }
        // 逆向きも。書き出しても、被覆が足りなければ完成ではない。
        let s = ProjectState {
            coverage: CoverageState::Incomplete,
            handoff: HandoffState::Exported,
        };
        assert!(!s.is_complete());
    }
    /// 複製は派生として親子関係を残す（`TR-PKG-46`）。
    #[test]
    fn derive_keeps_the_lineage() {
        let lib = Library::open(tmp("derive")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        let (_ledger, n) = ledger_with_rows(&parent);
        fs::write(parent.audio_dir().join("a.wav"), b"wav").expect("書けること");

        let child = lib
            .derive(&parent, "こえるちゃん（低め）")
            .expect("複製できること");
        let m = child.read_manifest().expect("読めること");

        assert_eq!(m.derived_from, Some(parent.id()));
        assert_eq!(m.display_name, "こえるちゃん（低め）");
        assert_ne!(child.id(), parent.id());
        // 親の台帳は開いたままで、行は WAL にしか無い。
        assert_eq!(rows_in(&child.db_path()), Some(n));
        assert_eq!(
            fs::read(child.audio_dir().join("a.wav")).expect("読める"),
            b"wav"
        );
    }

    fn sha256(path: &Path) -> Vec<u8> {
        use sha2::{Digest as _, Sha256};
        Sha256::digest(fs::read(path).expect("読めること")).to_vec()
    }

    /// 多音階の音声は音高ごとのディレクトリにある（`TR-REC-36`）。 入れ子ごと、
    /// バイトを変えずに写す。
    #[test]
    fn derive_copies_audio_per_tone() {
        let lib = Library::open(tmp("derive-tones")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        let (_ledger, _) = ledger_with_rows(&parent);
        let files = [
            format!("{}/あ_1.wav", crate::tone::name(60)),
            format!("{}/あ_2.wav", crate::tone::name(60)),
            format!("{}/あ_1.frq", crate::tone::name(60)),
            format!("{}/い_1.wav", crate::tone::name(67)),
            "oto.ini".to_owned(),
        ];
        for (i, rel) in files.iter().enumerate() {
            let path = parent.audio_dir().join(rel);
            fs::create_dir_all(path.parent().expect("親がある")).expect("作れること");
            // 中身が互いに違うこと。 同じだと、取り違えて写しても気づけない。
            let bytes: Vec<u8> = (0..65_536_usize)
                .map(|k| ((k * 31 + i * 7) % 251) as u8)
                .collect();
            fs::write(&path, bytes).expect("書けること");
        }
        // 書きかけのテイク。 確定していないので資産ではない。
        let partial = format!("{}/う_1.wav.part", crate::tone::name(60));
        fs::write(parent.audio_dir().join(&partial), b"half").expect("書けること");

        let child = lib.derive(&parent, "派生").expect("複製できること");

        for rel in &files {
            assert_eq!(
                sha256(&child.audio_dir().join(rel)),
                sha256(&parent.audio_dir().join(rel)),
                "{rel} がバイト単位で同じであること"
            );
        }
        assert!(
            !child.audio_dir().join(&partial).exists(),
            "書きかけを写さないこと"
        );
        assert!(
            !names_in(lib.root()).iter().any(|n| n.ends_with(".part")),
            "組み立て中の派生が残らないこと"
        );
    }

    /// 複製が途中で落ちても、欠けたプロジェクトを一覧に出さない。
    #[test]
    fn failed_derive_leaves_no_project() {
        let lib = Library::open(tmp("derive-fail")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        fs::write(parent.audio_dir().join("a.wav"), b"wav").expect("書けること");
        fs::write(parent.db_path(), "SQLite ではない").expect("書けること");

        let e = lib.derive(&parent, "派生").expect_err("落ちること");
        assert_eq!(koeru_failure::Failure::code(&e), "project.db_copy");
        assert_eq!(
            names_in(lib.root()),
            BTreeSet::from([parent.id().to_string()]),
            "親だけが残り、組み立て中の派生も残らないこと"
        );
    }

    /// 片方を消しても、もう片方の音は残る（`TR-PKG-39`）。
    #[test]
    fn derived_audio_is_independent_of_the_parent() {
        let lib = Library::open(tmp("derive2")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        fs::write(parent.audio_dir().join("a.wav"), b"wav").expect("書けること");
        let child = lib.derive(&parent, "派生").expect("複製できること");

        fs::remove_dir_all(parent.root()).expect("消せること");
        assert_eq!(
            fs::read(child.audio_dir().join("a.wav")).expect("読める"),
            b"wav"
        );
    }

    /// 読めない親 UUID を「親なし」に倒さない。
    #[test]
    fn unreadable_lineage_is_an_error_not_a_silent_orphan() {
        let e = Manifest::from_toml(
            "version = 1\ndisplay_name = 'x'\nmethod = 'single'\nitem_count = 1\nderived_from = 'ではない'",
        )
        .expect_err("拒むこと");
        assert!(matches!(
            e,
            ProjectError::ManifestField {
                field: "derived_from"
            }
        ));
    }

    #[test]
    fn lineage_round_trips_through_the_manifest() {
        let lib = Library::open(tmp("derive3")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        let child = lib.derive(&parent, "派生").expect("複製できること");
        assert_eq!(
            lib.open_project(child.id())
                .expect("開けること")
                .read_manifest()
                .expect("読めること")
                .derived_from,
            Some(parent.id())
        );
    }
}
