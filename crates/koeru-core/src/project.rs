//! プロジェクトのディレクトリ（`TR-PKG-37`, `TR-PKG-38`, `TR-PLT-23`, `TR-PKG-40`）。
//!
//! **oto.ini を作業ファイルとして置かない**（`TR-PKG-40`）。
//! タイミング5値とエイリアスは DB を正とし、oto.ini は書き出し時に作る派生物。
//! ここに置くと、DB と食い違ったまま外部ツールに編集される。
//!
//! **プロジェクトはディレクトリで、名前は不変の UUID。** 表示名を変えても
//! ディレクトリ名は動かない。表示名に FS 上不正な文字や CP932 で表現できない
//! 文字が入っても、保存と再開は壊れない（`TR-PKG-37`）。
//!
//! **副作用として、ライブラリ配下を人が見ても中身が判別できない。**
//! だから各プロジェクトの直下に人間可読な manifest を平文で置く。
//! これは飾りではなく、UUID 名の代償として要件が課している埋め合わせ。
//!
//! ```text
//! <library>/
//!   0193f0c4-.../          ← 不変の UUID
//!     manifest.toml        ← 人間可読。表示名・方式・項目数
//!     project.db           ← 構造化データ（crate::db）
//!     audio/               ← 録音 WAV。**不変資産**（TR-PKG-39）
//!     renders/             ← 試唱キャッシュ。捨ててよい
//!     exports/             ← 生成済みパッケージ
//!     snapshots/           ← 破壊的操作の直前の DB と manifest（TR-PKG-43）
//! ```

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, value};
use uuid::Uuid;

/// manifest の書式版。**読めない版を黙って読まない。**
pub const MANIFEST_VERSION: i64 = 1;

/// プロジェクトのディレクトリを扱うときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("入出力に失敗した")]
    Io(#[from] std::io::Error),

    /// manifest が TOML として読めない。
    #[error("manifest を解析できない")]
    ManifestSyntax(#[source] toml_edit::TomlError),

    /// manifest に要る鍵が無い、または型が違う。
    #[error("manifest の {field} が読めない")]
    ManifestField { field: &'static str },

    /// 知らない版の manifest。**推測で読まない。**
    #[error("manifest の版 {found} は扱えない（このビルドは {MANIFEST_VERSION}）")]
    ManifestVersion { found: i64 },

    /// ディレクトリ名が UUID でない。
    #[error("プロジェクトのディレクトリ名が UUID でない")]
    NotAProjectDir,

    /// 知らない方式名。
    #[error("方式 {found} を知らない")]
    UnknownMethod { found: String },
}

impl ProjectError {
    /// 送信してよい種別文字列（`rust-conventions`）。
    ///
    /// **`Display` は送らない。** 表示名やパスが混じる。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "project.io",
            Self::ManifestSyntax(_) => "project.manifest_syntax",
            Self::ManifestField { .. } => "project.manifest_field",
            Self::ManifestVersion { .. } => "project.manifest_version",
            Self::NotAProjectDir => "project.not_a_project_dir",
            Self::UnknownMethod { .. } => "project.unknown_method",
        }
    }
}

type Result<T> = std::result::Result<T, ProjectError>;

/// 収録方式。
///
/// **M2 で生成できるのは `Single` だけ**（[`crate::reclist::generate_single`]）。
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
    /// manifest に書く名前。**表示用の日本語ではなく、機械が読む安定した識別子。**
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Sequential => "sequential",
            Self::Cvvc => "cvvc",
            Self::MultiPitchSequential => "multi_pitch_sequential",
        }
    }

    fn parse(s: &str) -> Result<Self> {
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

/// 手渡しの状態（`TR-PKG-33`）。**完成判定はこれを参照しない。**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandoffState {
    /// 一度も書き出していない。
    NotExported,
    /// 書き出したことがある。
    Exported,
}

/// カバレッジ側の状態（`TR-PKG-33`, `TR-PKG-34`）。
///
/// **完成 = 必須エイリアス表を 100% 被覆し、全 oto が検証を通り、表示名がある状態。**
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

/// プロジェクトの状態。**2軸は直交する**（`TR-PKG-33`）。
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
    /// **`handoff` を一切見ない**（`TR-PKG-33`）。ZIP を1度も作っていなくても
    /// 完成は完成（`TR-PKG-36`）。
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        matches!(self.coverage, CoverageState::Complete)
    }
}

/// 完成状態を決める（`TR-PKG-34`）。
///
/// 引数は3つとも呼び出し側が DB から引く。**ここでは合成規則だけを持つ。**
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
    /// 表示名。**FS 上不正な文字や CP932 外の文字が入ってよい。**
    pub display_name: String,
    /// 収録方式。
    pub method: Method,
    /// 録音リストの項目数。
    pub item_count: u32,
    /// 複製元（`TR-PKG-46`）。
    ///
    /// **複製は「派生」として親子関係を残す。** 同名・同内容の別プロジェクトが
    /// 並ぶと、どちらが後のものか分からなくなる。
    pub derived_from: Option<Uuid>,
}

/// manifest の先頭に置く説明。**これが「人が見て判別できる」の実体。**
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

        // **読めない親 UUID は「親なし」に倒さず、失敗として返す。**
        // 静かに独立プロジェクトになると、派生関係が消えたことに誰も気づかない。
        let derived_from = match doc.get("derived_from") {
            None => None,
            Some(item) => Some(item.as_str().and_then(|s| Uuid::parse_str(s).ok()).ok_or(
                ProjectError::ManifestField {
                    field: "derived_from",
                },
            )?),
        };

        Ok(Self {
            display_name,
            method,
            item_count,
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

    /// 構造化データ。
    #[must_use]
    pub fn db_path(&self) -> PathBuf {
        self.root.join("project.db")
    }

    /// 録音 WAV。**不変資産**（`TR-PKG-39`）。
    #[must_use]
    pub fn audio_dir(&self) -> PathBuf {
        self.root.join("audio")
    }

    /// 試唱キャッシュ。**消しても再生成できる。**
    #[must_use]
    pub fn renders_dir(&self) -> PathBuf {
        self.root.join("renders")
    }

    /// 生成済みパッケージ。**過去のものを上書きしない**（`TR-PKG-44`）。
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
    #[tracing::instrument(skip(self), err)]
    pub fn read_manifest(&self) -> Result<Manifest> {
        Manifest::from_toml(&fs::read_to_string(self.manifest_path())?)
    }

    /// manifest を書く。**一時ファイル → fsync → rename**（`TR-PKG-41`）。
    ///
    /// 途中で落ちても、部分的に書かれた manifest は残らない。
    #[tracing::instrument(skip(self, m), err)]
    pub fn write_manifest(&self, m: &Manifest) -> Result<()> {
        write_atomically(&self.manifest_path(), m.to_toml().as_bytes())
    }

    /// 破壊的操作の直前に控えを取る（`TR-PKG-43`）。
    ///
    /// **WAV は複製しない。** DB と manifest だけを複製し、WAV は元を参照する。
    /// 3時間の録音を操作のたびに二重化したら、ディスクがいくつあっても足りない。
    ///
    /// `seq` は呼び出し側が単調増加で与える。`label` は操作の名前
    /// （`realign` / `downgrade_export` / `bulk_alias` / `delete_items` / `change_method`）。
    #[tracing::instrument(skip(self), err)]
    pub fn take_snapshot(&self, seq: u32, label: &str) -> Result<PathBuf> {
        let dir = self.snapshots_dir().join(format!("{seq:06}-{label}"));
        fs::create_dir_all(&dir)?;
        fs::copy(self.db_path(), dir.join("project.db"))?;
        fs::copy(self.manifest_path(), dir.join("manifest.toml"))?;
        // ディレクトリエントリを永続化する。**中身だけ fsync しても、
        // ディレクトリが飛べば控えは無い。**
        fsync_dir(&self.snapshots_dir())?;
        Ok(dir)
    }

    /// 取ってある控えを古い順に挙げる。
    #[tracing::instrument(skip(self), err)]
    pub fn snapshots(&self) -> Result<Vec<PathBuf>> {
        let dir = self.snapshots_dir();
        if !dir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(std::result::Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| e.path())
            .collect();
        out.sort();
        Ok(out)
    }
}

/// アプリが管理するライブラリ（`TR-PKG-37`）。
///
/// **利用者にフォルダ操作を要求しない**（`TR-PKG-45`）。保存先の選択も、
/// WAV のリネームも、バックアップフォルダ作りもここが引き受ける。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Library {
    root: PathBuf,
}

impl Library {
    /// ライブラリを開く。無ければ作る。
    #[tracing::instrument(skip(root), err)]
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// ライブラリの根。**通常モードの画面には出さない**（`TR-PKG-45`）。
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// プロジェクトを作る。**ディレクトリ名は UUID で、以後変えない。**
    #[tracing::instrument(skip(self, m), err)]
    pub fn create(&self, m: &Manifest) -> Result<ProjectDir> {
        let id = Uuid::new_v4();
        let dir = ProjectDir {
            id,
            root: self.root.join(id.to_string()),
        };
        for d in [
            dir.root.clone(),
            dir.audio_dir(),
            dir.renders_dir(),
            dir.exports_dir(),
            dir.snapshots_dir(),
        ] {
            fs::create_dir_all(&d)?;
        }
        dir.write_manifest(m)?;
        fsync_dir(&self.root)?;
        Ok(dir)
    }

    /// プロジェクトを複製して派生を作る（`TR-PKG-46`）。
    ///
    /// **親子関係を manifest に残す。** 同じライブラリに同名・同内容のものが
    /// 並んでも、どちらから出たかは辿れる。
    ///
    /// WAV は複製する。**元は不変資産なので参照でも足りるが、片方を消したときに
    /// もう片方の音が消えるのは説明がつかない**（`TR-PKG-39`）。
    #[tracing::instrument(skip(self, parent), err)]
    pub fn derive(&self, parent: &ProjectDir, display_name: &str) -> Result<ProjectDir> {
        let mut m = parent.read_manifest()?;
        m.display_name = display_name.to_owned();
        m.derived_from = Some(parent.id());

        let child = self.create(&m)?;
        // DB は丸ごと引き継ぐ（録音も原音設定も引き継ぐのが「複製」）。
        if parent.db_path().is_file() {
            fs::copy(parent.db_path(), child.db_path())?;
        }
        for entry in fs::read_dir(parent.audio_dir())? {
            let src = entry?.path();
            if !src.is_file() {
                continue;
            }
            let Some(name) = src.file_name() else {
                continue;
            };
            fs::copy(&src, child.audio_dir().join(name))?;
        }
        Ok(child)
    }

    /// 既にあるプロジェクトを開く。
    #[tracing::instrument(skip(self), err)]
    pub fn open_project(&self, id: Uuid) -> Result<ProjectDir> {
        let root = self.root.join(id.to_string());
        if !root.is_dir() {
            return Err(ProjectError::NotAProjectDir);
        }
        Ok(ProjectDir { id, root })
    }

    /// ライブラリの中身を挙げる。
    ///
    /// **manifest が読めないものは飛ばさず、失敗として返す。** 一覧から静かに
    /// 消えると、利用者は「プロジェクトが無くなった」と受け取る。
    #[tracing::instrument(skip(self), err)]
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

/// 一時ファイルへ書いて fsync し、アトミックな rename で置き換える（`TR-PKG-41`）。
///
/// **rename が成功するまで、元のファイルはそのまま。** 途中で落ちても、
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

/// ディレクトリエントリを永続化する。
///
/// **ファイルを fsync しても、ディレクトリを fsync しないと rename が飛ぶ。**
/// Windows にはディレクトリを開く経路が無いので、そこでは何もしない
/// （NTFS のメタデータ更新はジャーナルで守られる）。
fn fsync_dir(path: &Path) -> Result<()> {
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

    /// テスト用の一時ディレクトリ。**プロセス ID と連番で衝突を避ける。**
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
        }
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

        // **ディレクトリ名は UUID**（TR-PKG-37）。
        let name = p.root().file_name().and_then(|s| s.to_str()).expect("名前");
        assert_eq!(Uuid::parse_str(name).expect("UUID であること"), p.id());
    }

    #[test]
    fn manifest_round_trips() {
        let lib = Library::open(tmp("round")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        assert_eq!(p.read_manifest().expect("読めること"), manifest());
    }

    /// **manifest は人が読めること**（TR-PKG-37）。
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

    /// **表示名に FS 上不正な文字や CP932 外の文字が入っても壊れない**（TR-PKG-37）。
    #[test]
    fn display_name_may_be_hostile() {
        let lib = Library::open(tmp("hostile")).expect("開けること");
        let hostile = Manifest {
            // スラッシュ・コロン・NUL 以外の制御文字・絵文字・CP932 外の漢字。
            display_name: "a/b:c*d?\u{7}🎤𠮷 \"quoted\"".to_owned(),
            method: Method::Sequential,
            item_count: 7,
            derived_from: None,
        };
        let p = lib.create(&hostile).expect("作れること");
        assert_eq!(p.read_manifest().expect("読めること"), hostile);

        // ディレクトリ名は表示名に一切影響されない。
        let name = p.root().file_name().and_then(|s| s.to_str()).expect("名前");
        assert!(Uuid::parse_str(name).is_ok());
    }

    /// **改名でディレクトリ名を変えない**（TR-PKG-37）。
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

    /// **部分的に書かれた manifest を残さない**（TR-PKG-41）。
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
    }

    #[test]
    fn unknown_method_is_refused() {
        let e = Manifest::from_toml(
            "version = 1\ndisplay_name = 'x'\nmethod = 'vcv-ish'\nitem_count = 1",
        )
        .expect_err("拒むこと");
        assert_eq!(e.kind(), "project.unknown_method");
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

    /// **読めない manifest を一覧から静かに消さない。**
    #[test]
    fn list_reports_broken_manifests_instead_of_hiding_them() {
        let lib = Library::open(tmp("broken")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        fs::write(p.manifest_path(), "これは TOML ではない = = =").expect("書けること");

        let listed = lib.list().expect("挙げられること");
        assert_eq!(listed.len(), 1, "一覧から消えないこと");
        assert!(listed[0].1.is_err(), "失敗として返ること");
    }

    /// **控えは WAV を複製しない**（TR-PKG-43）。
    #[test]
    fn snapshot_copies_the_db_but_not_the_audio() {
        let lib = Library::open(tmp("snap")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        fs::write(p.db_path(), b"pretend-db").expect("書けること");
        // 3時間ぶんのつもりの WAV。
        fs::write(p.audio_dir().join("a.wav"), vec![0_u8; 4096]).expect("書けること");

        let dir = p.take_snapshot(1, "realign").expect("取れること");

        assert!(dir.join("project.db").is_file());
        assert!(dir.join("manifest.toml").is_file());
        assert!(!dir.join("audio").exists(), "WAV を複製しないこと");

        // 元の WAV はそのまま残る（参照する側）。
        assert!(p.audio_dir().join("a.wav").is_file());
    }

    #[test]
    fn snapshots_are_listed_oldest_first() {
        let lib = Library::open(tmp("snaps")).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        fs::write(p.db_path(), b"db").expect("書けること");

        for (seq, label) in [(1, "realign"), (2, "bulk_alias"), (10, "change_method")] {
            p.take_snapshot(seq, label).expect("取れること");
        }
        let got: Vec<String> = p
            .snapshots()
            .expect("挙げられること")
            .iter()
            .filter_map(|d| d.file_name().and_then(|s| s.to_str()).map(str::to_owned))
            .collect();
        assert_eq!(
            got,
            [
                "000001-realign",
                "000002-bulk_alias",
                "000010-change_method"
            ],
            "連番は桁を揃えて辞書順が時系列と一致すること"
        );
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

    /// **完成判定は書き出し履歴を見ない**（TR-PKG-33, TR-PKG-36）。
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
    /// **複製は派生として親子関係を残す**（TR-PKG-46）。
    #[test]
    fn derive_keeps_the_lineage() {
        let lib = Library::open(tmp("derive")).expect("開けること");
        let parent = lib.create(&manifest()).expect("作れること");
        fs::write(parent.db_path(), b"db").expect("書けること");
        fs::write(parent.audio_dir().join("a.wav"), b"wav").expect("書けること");

        let child = lib
            .derive(&parent, "こえるちゃん（低め）")
            .expect("複製できること");
        let m = child.read_manifest().expect("読めること");

        assert_eq!(m.derived_from, Some(parent.id()));
        assert_eq!(m.display_name, "こえるちゃん（低め）");
        assert_ne!(child.id(), parent.id());
        assert_eq!(fs::read(child.db_path()).expect("読める"), b"db");
        assert_eq!(
            fs::read(child.audio_dir().join("a.wav")).expect("読める"),
            b"wav"
        );
    }

    /// **片方を消しても、もう片方の音は残る**（TR-PKG-39）。
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

    /// **読めない親 UUID を「親なし」に倒さない。**
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
