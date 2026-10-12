//! D07b の Component Inventory の試験。 fixture の ID は `fix` を領域にする
//! （`check-references` が拾わない綴り）。 実在する meta と manifest を読ませる試験が最後に1本ある。

use std::collections::BTreeMap;
use std::path::Path;

use super::manifest::strip_trailing_commas;
use super::*;
use crate::knowledge::design::Design;
use crate::repo::RepoView;

const DECISION: &str = "schema = 'decision'
id = 'DEC-fix-001'
title = '採否'
status = 'accepted'
owner = 'x'
options = ['a', 'b']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
considers_evidence = ['EVID-fix-002']
";

/// 部品に直接触れている調査。
const EVIDENCE_LICENSE: &str = "schema = 'evidence'
id = 'EVID-fix-001'
title = 'license'
kind = 'license_audit'
source = '出典'
provenance = '出どころ'
confidence = 'Fact'
affects_components = ['CMP-fix-001']
";

/// 判断が考慮した調査。 部品には直接触れていない。
const EVIDENCE_MEASURE: &str = "schema = 'evidence'
id = 'EVID-fix-002'
title = '測定'
kind = 'measurement'
source = '出典'
provenance = '出どころ'
confidence = 'Fact'
";

const COMPONENTS: &str = "schema = 'component-ledger'

[[component]]
id = 'CMP-fix-001'
name = 'serde'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'DEC-fix-001'
domains = ['fix']

[[component]]
id = 'CMP-fix-002'
name = 'tokio'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'DEC-fix-404'
domains = ['fix']

[[component]]
id = 'CMP-fix-003'
name = 'rusqlite'
purpose = '用途'
license = 'MIT'
status = '不適'
decided_by = 'DEC-fix-001'
domains = ['fix']

[[component]]
id = 'CMP-fix-004'
name = 'Rust 1.97'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'EVID-fix-001'
domains = ['fix']

[[component]]
id = 'CMP-fix-005'
name = 'itoa'
purpose = '用途'
license = 'MIT'
status = '採用'
domains = ['fix']

[[component]]
id = 'CMP-fix-006'
name = 'world'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'DEC-fix-001'
domains = ['fix']

[[component]]
id = 'CMP-fix-007'
name = 'React / Tailwind CSS（v19）'
purpose = '用途'
license = 'MIT'
status = '採用候補'
domains = ['fix']

[[component]]
id = 'CMP-fix-008'
name = 'react-dom 19.3.0'
purpose = '用途'
license = 'MIT'
status = '採用'
domains = ['fix']
";

const WORKSPACE_CARGO: &str = "[workspace]
members = ['crates/*']

[workspace.dependencies]
serde = '1.0'
rusqlite = '0.30'
fix-internal = { path = 'crates/fix-internal' }
";

const APP_CARGO: &str = "[package]
name = 'app'

[dependencies]
serde.workspace = true
anyhow = '1'
fix-internal.workspace = true

[dev-dependencies]
rusqlite = { workspace = true }

[target.'cfg(windows)'.dependencies]
windows_sys = '0.1'
";

const INTERNAL_CARGO: &str = "[package]
name = 'fix-internal'
";

const CARGO_LOCK: &str = "version = 4

[[package]]
name = 'app'
version = '0.0.0'

[[package]]
name = 'serde'
version = '1.0.1'
source = 'registry+https://example.invalid/index'

[[package]]
name = 'rusqlite'
version = '0.30.0'
source = 'registry+https://example.invalid/index'

[[package]]
name = 'anyhow'
version = '1.0.9'
source = 'registry+https://example.invalid/index'

[[package]]
name = 'itoa'
version = '1.0.0'
source = 'registry+https://example.invalid/index'

[[package]]
name = 'windows-sys'
version = '0.1.0'
source = 'registry+https://example.invalid/index'
";

const TOOLCHAIN: &str = "[toolchain]
channel = '1.98.1'
";

const PACKAGE_JSON: &str = "{
  \"name\": \"@fix/ui\",
  \"dependencies\": { \"react\": \"19.3.0\", \"react-dom\": \"19.3.0\", \"@fix/local\": \"workspace:*\" },
  \"devDependencies\": { \"vitest\": \"4.0.0\" }
}
";

const BUN_LOCK: &str = "{
  \"lockfileVersion\": 2,
  \"packages\": {
    \"react\": [\"react@19.3.0\", \"\", {}, \"sha512-x\"],
    \"react-dom\": [\"react-dom@19.4.0\", \"\", {}, \"sha512-y\"],
    \"@scope/pkg\": [\"@scope/pkg@1.2.3\", \"\", {}, \"sha512-z\"],
  },
}
";

const GITMODULES: &str = "[submodule \"crates/fix/vendor/world\"]
\tpath = crates/fix/vendor/world
\turl = https://example.invalid/world
";

const FLAKE_LOCK: &str = "{
  \"nodes\": {
    \"nixpkgs\": {
      \"locked\": { \"rev\": \"abc123\", \"type\": \"github\" },
      \"original\": { \"ref\": \"nixos-unstable\", \"type\": \"github\" }
    },
    \"root\": { \"inputs\": { \"nixpkgs\": \"nixpkgs\", \"follower\": [\"nixpkgs\"] } }
  },
  \"root\": \"root\",
  \"version\": 7
}
";

fn files() -> BTreeMap<String, String> {
    [
        ("meta/decisions/DEC-fix-001.toml", DECISION),
        ("meta/evidence/EVID-fix-001.toml", EVIDENCE_LICENSE),
        ("meta/evidence/EVID-fix-002.toml", EVIDENCE_MEASURE),
        ("meta/evidence/components.toml", COMPONENTS),
        ("Cargo.toml", WORKSPACE_CARGO),
        ("crates/app/Cargo.toml", APP_CARGO),
        ("crates/fix-internal/Cargo.toml", INTERNAL_CARGO),
        ("Cargo.lock", CARGO_LOCK),
        ("rust-toolchain.toml", TOOLCHAIN),
        ("ui/package.json", PACKAGE_JSON),
        ("ui/bun.lock", BUN_LOCK),
        (".gitmodules", GITMODULES),
        ("flake.lock", FLAKE_LOCK),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_owned(), b.to_owned()))
    .collect()
}

fn inventory_of(files: &BTreeMap<String, String>) -> ComponentInventory {
    let design = Design::load(&RepoView::in_memory(Path::new("/fixture"), files.clone()));
    let graph = design.graph();
    ComponentInventory::build(
        &design,
        &graph,
        &RepoView::in_memory(Path::new("/fixture"), files.clone()),
    )
}

fn with(path: &str, body: &str) -> BTreeMap<String, String> {
    let mut f = files();
    f.insert(path.to_owned(), body.to_owned());
    f
}

fn id(raw: &str) -> Id {
    raw.parse().expect("ID の形")
}

fn pkg(ecosystem: Ecosystem, name: &str) -> PackageRef {
    PackageRef {
        ecosystem,
        name: name.to_owned(),
    }
}

fn entry<'a>(inv: &'a ComponentInventory, raw: &str) -> &'a ComponentEntry {
    let wanted = id(raw);
    inv.components
        .iter()
        .find(|c| c.id == wanted)
        .expect("台帳の行")
}

fn package<'a>(inv: &'a ComponentInventory, ecosystem: Ecosystem, name: &str) -> &'a Package {
    // 綴りは最初に見たほうで持つ。 `_` と `-` の違いは問わない。
    let wanted = manifest::normalize(name);
    inv.packages
        .iter()
        .find(|p| {
            p.package.ecosystem == ecosystem && manifest::normalize(&p.package.name) == wanted
        })
        .expect("repository の package")
}

#[test]
fn package_の存在と版は_manifest_と_lock_から得る() {
    let inv = inventory_of(&files());
    // Cargo。 workspace の宣言とメンバーの宣言が1件にまとまり、lock の版を持つ。
    let serde = package(&inv, Ecosystem::Cargo, "serde");
    assert_eq!(serde.locked_versions, vec!["1.0.1".to_owned()]);
    assert_eq!(serde.declarations.len(), 2);
    assert!(serde.declarations.iter().any(|d| d.manifest == "Cargo.toml"
        && d.section == Section::WorkspaceDependencies
        && d.requirement.as_deref() == Some("1.0")));
    assert!(
        serde
            .declarations
            .iter()
            .any(|d| d.manifest == "crates/app/Cargo.toml"
                && d.requirement.as_deref() == Some("workspace"))
    );
    // dev と target 固有の宣言も読む。 `_` と `-` は同じ名前。
    let rusqlite = package(&inv, Ecosystem::Cargo, "rusqlite");
    assert!(
        rusqlite
            .declarations
            .iter()
            .any(|d| d.section == Section::DevDependencies)
    );
    let windows = package(&inv, Ecosystem::Cargo, "windows_sys");
    assert_eq!(windows.locked_versions, vec!["0.1.0".to_owned()]);
    assert!(windows.is_declared());
    // lock にしか無い package は、宣言していない。
    let itoa = package(&inv, Ecosystem::Cargo, "itoa");
    assert!(!itoa.is_declared());
    assert_eq!(itoa.locked_versions, vec!["1.0.0".to_owned()]);
    // toolchain・Bun・Nix・submodule。
    assert_eq!(
        package(&inv, Ecosystem::RustToolchain, "rust").locked_versions,
        vec!["1.98.1".to_owned()]
    );
    assert_eq!(
        package(&inv, Ecosystem::Bun, "react").locked_versions,
        vec!["19.3.0".to_owned()]
    );
    assert_eq!(
        package(&inv, Ecosystem::Bun, "@scope/pkg").locked_versions,
        vec!["1.2.3".to_owned()]
    );
    let nixpkgs = package(&inv, Ecosystem::Nix, "nixpkgs");
    assert_eq!(nixpkgs.locked_versions, vec!["abc123".to_owned()]);
    assert_eq!(
        nixpkgs.declarations[0].requirement.as_deref(),
        Some("nixos-unstable")
    );
    let world = package(&inv, Ecosystem::Submodule, "world");
    assert!(world.locked_versions.is_empty());
    assert_eq!(
        world.declarations[0].requirement.as_deref(),
        Some("https://example.invalid/world")
    );
    assert!(inv.manifest_issues.is_empty());
}

#[test]
fn この_repo_の中の_crate_と_workspace_の_package_は取り込んだ部品にならない() {
    let inv = inventory_of(&files());
    let names: Vec<&str> = inv
        .packages
        .iter()
        .map(|p| p.package.name.as_str())
        .collect();
    for internal in ["app", "fix-internal", "@fix/local"] {
        assert!(!names.contains(&internal), "{internal}");
    }
    // `follows`（配列）の input は、それ自身の版を持たない。
    assert!(!names.contains(&"follower"));
}

#[test]
fn 読めない_manifest_は落とさず問題として出る() {
    let inv = inventory_of(&with("crates/app/Cargo.toml", "[package\nname ="));
    assert_eq!(
        inv.manifest_issues,
        vec![ManifestIssue {
            path: "crates/app/Cargo.toml".to_owned(),
            reason: IssueReason::Malformed,
        }]
    );
    // ほかの manifest は読めている。
    assert!(inv.sources.contains(&"Cargo.lock".to_owned()));
    assert!(!inv.sources.contains(&"crates/app/Cargo.toml".to_owned()));
}

#[test]
fn bun_lock_の末尾のカンマは文字列の中を壊さず除く() {
    let stripped = strip_trailing_commas("{\"a\": [1, 2,], \"b\": \"x,]\", }");
    assert_eq!(stripped, "{\"a\": [1, 2], \"b\": \"x,]\" }");
    assert!(serde_json::from_str::<serde_json::Value>(&stripped).is_ok());
}

#[test]
fn 台帳の名前から_package_の名前と版を取り出す() {
    let (keys, versions) = name_keys("Tailwind CSS");
    assert!(keys.contains("tailwindcss"));
    assert!(versions.is_empty());

    let (keys, versions) = name_keys("tracing ＋ tracing-subscriber");
    assert!(keys.contains("tracing") && keys.contains("tracing-subscriber"));
    assert!(versions.is_empty());

    // スコープ付きの名前は崩さない。 それ以外の `/` は区切る。
    let (keys, _) = name_keys("@shadcn/lint");
    assert!(keys.contains("@shadcn/lint"));
    let (keys, _) = name_keys("React / TanStack Start（SPA モード）");
    assert!(keys.contains("react") && keys.contains("tanstack"));

    // 版は照合の語にしない。
    let (keys, versions) = name_keys("MFA 日本語辞書 v3.0.0");
    assert_eq!(versions, vec!["3.0.0".to_owned()]);
    assert!(!keys.contains("v3.0.0") && keys.contains("mfa"));
    let (_, versions) = name_keys("Rust 1.98 / edition 2024");
    assert_eq!(versions, vec!["1.98".to_owned()]);
}

#[test]
fn 台帳の行は_manifest_にあるか_lock_だけか_無いかが分かれる() {
    let inv = inventory_of(&files());
    assert_eq!(entry(&inv, "CMP-fix-001").presence, Presence::Declared);
    assert_eq!(
        entry(&inv, "CMP-fix-001").packages,
        vec![pkg(Ecosystem::Cargo, "serde")]
    );
    // 推移的な依存。
    assert_eq!(entry(&inv, "CMP-fix-005").presence, Presence::LockedOnly);
    // どこにも無い。
    assert_eq!(entry(&inv, "CMP-fix-002").presence, Presence::Absent);
    assert!(entry(&inv, "CMP-fix-002").packages.is_empty());
    // submodule は宣言だけで、版は無い。
    assert_eq!(entry(&inv, "CMP-fix-006").presence, Presence::Declared);
}

#[test]
fn 台帳と_manifest_の食い違い3種が値で出る() {
    let inv = inventory_of(&files());
    // 台帳にあるが manifest に無い（採用）。
    assert!(
        inv.discrepancies
            .contains(&Discrepancy::AdoptedNotInManifest {
                component: id("CMP-fix-002")
            })
    );
    // manifest にあるが台帳に無い。 直接の宣言だけで、推移的な依存や内部の crate は出ない。
    for name in ["anyhow", "windows-sys"] {
        assert!(
            inv.discrepancies
                .contains(&Discrepancy::DeclaredNotInLedger {
                    package: pkg(Ecosystem::Cargo, name)
                }),
            "{name}"
        );
    }
    assert!(!inv.discrepancies.iter().any(|d| matches!(
        d,
        Discrepancy::DeclaredNotInLedger { package }
            if package.name == "itoa" || package.name == "fix-internal"
    )));
    // 台帳の名前に書かれた版が、lock・toolchain の版と合わない。
    assert!(inv.discrepancies.contains(&Discrepancy::VersionDiffers {
        component: id("CMP-fix-004"),
        package: pkg(Ecosystem::RustToolchain, "rust"),
        ledger: vec!["1.97".to_owned()],
        repository: vec!["1.98.1".to_owned()],
    }));
    // 採用でない部品の食い違いは、採用の不在としては出ない。
    assert!(!inv.discrepancies.iter().any(|d| matches!(
        d,
        Discrepancy::AdoptedNotInManifest { component } if *component == id("CMP-fix-003")
    )));
}

#[test]
fn 台帳の版は上位の書き方なら合っているとみなす() {
    assert!(version_covers("1.98", "1.98.1"));
    assert!(version_covers("1.98.1", "1.98.1"));
    assert!(!version_covers("1.9", "1.98.1"));
    assert!(!version_covers("1.97", "1.98.1"));
    // 版を持たない submodule・版が合っている部品は、食い違いに出ない。
    let inv = inventory_of(&files());
    assert!(!inv.discrepancies.iter().any(|d| matches!(
        d,
        Discrepancy::VersionDiffers { component, .. } if *component == id("CMP-fix-006")
    )));
    let fixed = COMPONENTS.replace("Rust 1.97", "Rust 1.98");
    let inv = inventory_of(&with("meta/evidence/components.toml", &fixed));
    assert!(!inv.discrepancies.iter().any(|d| matches!(
        d,
        Discrepancy::VersionDiffers { component, .. } if *component == id("CMP-fix-004")
    )));
}

#[test]
fn 版が1つでも合えば_複数の版を持つ_package_は食い違いにならない() {
    let lock = format!(
        "{CARGO_LOCK}\n[[package]]\nname = 'serde'\nversion = '2.0.0'\nsource = 'registry+x'\n"
    );
    let comps = COMPONENTS.replace("name = 'serde'", "name = 'serde v2.0'");
    let mut f = with("Cargo.lock", &lock);
    f.insert("meta/evidence/components.toml".to_owned(), comps);
    let inv = inventory_of(&f);
    assert_eq!(
        package(&inv, Ecosystem::Cargo, "serde").locked_versions,
        vec!["1.0.1".to_owned(), "2.0.0".to_owned()]
    );
    assert!(!inv.discrepancies.iter().any(|d| matches!(
        d,
        Discrepancy::VersionDiffers { component, .. } if *component == id("CMP-fix-001")
    )));
}

#[test]
fn 不適の部品を_manifest_が宣言していれば出る() {
    let inv = inventory_of(&files());
    assert!(
        inv.discrepancies
            .contains(&Discrepancy::RejectedButDeclared {
                component: id("CMP-fix-003"),
                package: pkg(Ecosystem::Cargo, "rusqlite"),
            })
    );
    // lock にしか無いだけなら、宣言していない。 出ない。
    let comps = COMPONENTS.replace("name = 'rusqlite'", "name = 'itoa'");
    let inv = inventory_of(&with("meta/evidence/components.toml", &comps));
    assert!(
        !inv.discrepancies
            .iter()
            .any(|d| matches!(d, Discrepancy::RejectedButDeclared { .. }))
    );
}

#[test]
fn 採否の理由は判断へ戻り_戻れないものは戻り先が無いと出る() {
    let inv = inventory_of(&files());
    assert_eq!(
        entry(&inv, "CMP-fix-001").rationale,
        Rationale::Decision(id("DEC-fix-001"))
    );
    // 指す判断が無い。
    assert_eq!(
        entry(&inv, "CMP-fix-002").rationale,
        Rationale::Unresolved("DEC-fix-404".to_owned())
    );
    // 判断ではないものを指している。
    assert_eq!(
        entry(&inv, "CMP-fix-004").rationale,
        Rationale::NotDecision {
            id: id("EVID-fix-001"),
            kind: Kind::Evidence,
        }
    );
    // `decided_by` が無い。
    assert_eq!(
        entry(&inv, "CMP-fix-005").rationale,
        Rationale::NoneRecorded
    );
    // ID の形ではない。
    let comps = COMPONENTS.replace("decided_by = 'DEC-fix-404'", "decided_by = 'なし'");
    let inv = inventory_of(&with("meta/evidence/components.toml", &comps));
    assert_eq!(
        entry(&inv, "CMP-fix-002").rationale,
        Rationale::Unresolved("なし".to_owned())
    );
}

#[test]
fn license_と測定は調査へ戻る() {
    let inv = inventory_of(&files());
    // 部品を直接指す調査と、採否を決めた判断が考慮した調査の両方。 種類も付く。
    assert_eq!(
        entry(&inv, "CMP-fix-001").evidence,
        vec![
            EvidenceLink {
                evidence: id("EVID-fix-001"),
                kind: Some("license_audit".to_owned()),
                via: EvidenceVia::AffectsComponent,
            },
            EvidenceLink {
                evidence: id("EVID-fix-002"),
                kind: Some("measurement".to_owned()),
                via: EvidenceVia::ConsideredByDecision(id("DEC-fix-001")),
            },
        ]
    );
    // 判断に戻れない部品は、判断経由の調査を持たない。 何も無ければ空。
    assert!(entry(&inv, "CMP-fix-002").evidence.is_empty());
}

#[test]
fn 採否の語に依らず同じ名前の_package_を引く() {
    // `採用候補` の行も、名前が一致すれば引く（採否の判定は呼び側）。 語の一致は完全一致で、
    // `React` は引くが、一致しない語は引かない。
    let inv = inventory_of(&files());
    let candidate = entry(&inv, "CMP-fix-007");
    assert_eq!(candidate.adoption, Some(Adoption::Candidate));
    assert_eq!(candidate.packages, vec![pkg(Ecosystem::Bun, "react")]);
    assert!(candidate.ledger_versions.is_empty());
    // `react-dom 19.3.0` は名前の語が `react-dom`。 版は lock の 19.4.0 と合わない。
    assert!(inv.discrepancies.contains(&Discrepancy::VersionDiffers {
        component: id("CMP-fix-008"),
        package: pkg(Ecosystem::Bun, "react-dom"),
        ledger: vec!["19.3.0".to_owned()],
        repository: vec!["19.4.0".to_owned()],
    }));
}

/// 実在する meta と manifest を読ませる。 全部品が1行ずつ載り、取りこぼしが無い。
#[test]
fn 実在する_meta_と_manifest_の全部品が_view_を通る() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let repo = RepoView::working_tree(root);
    let design = Design::load(&repo);
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    let inv = ComponentInventory::build(&design, &graph, &repo);

    // 台帳の全行が、ID の重複なく1行ずつ載る。
    let ledger: Vec<Id> = design
        .objects()
        .filter(|o| o.as_component().is_some())
        .map(|o| o.id().clone())
        .collect();
    assert!(!ledger.is_empty());
    let mut listed: Vec<Id> = inv.components.iter().map(|c| c.id.clone()).collect();
    listed.sort();
    let mut expected = ledger;
    expected.sort();
    assert_eq!(listed, expected);

    // 読むべき manifest・lock が読めている。
    assert!(inv.manifest_issues.is_empty(), "{:?}", inv.manifest_issues);
    for required in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "flake.lock",
        ".gitmodules",
        "crates/koeru-app/ui/package.json",
        "crates/koeru-app/ui/bun.lock",
    ] {
        assert!(inv.sources.contains(&required.to_owned()), "{required}");
    }
    // 台帳の記述ではなく repository から得た版。 コミット済みの lock が持つ package が引ける。
    assert!(
        inv.packages
            .iter()
            .any(|p| p.package.ecosystem == Ecosystem::Cargo && !p.locked_versions.is_empty())
    );
    // この repo の中の crate は、取り込んだ部品に数えない。
    assert!(
        !inv.packages
            .iter()
            .any(|p| p.package.name.starts_with("koeru-"))
    );
    // 食い違いは値で並ぶだけ。 整列していて、同じ食い違いが2度出ない。
    let mut sorted = inv.discrepancies.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted, inv.discrepancies);
}
