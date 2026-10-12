//! リポジトリの manifest と lock から、package の存在と版を引く（D07b）。
//!
//! 部品台帳の記述を正にしない。 存在と版の正本は Cargo・Bun・toolchain・Nix の lock と
//! manifest、submodule の宣言で、ここはそれらを `RepoView` 越しに読む。 `cargo metadata` の
//! ような外部コマンドは起動しない（版を選んでも同じ答えになるように）。
//!
//! 読むのは宣言と lock に書かれていることだけ。 解決していない版は持たない。
//! submodule の版（コミット）は tree の gitlink にあり、`RepoView` からは読めないので
//! 空のまま。 読めなかったものは落とさず [`ManifestIssue`] に積む。

use std::collections::{BTreeMap, BTreeSet};

use crate::repo::RepoView;

/// package の出どころ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Ecosystem {
    Cargo,
    Bun,
    /// `rust-toolchain.toml` の `channel`。 名前は `rust` の1つだけ。
    RustToolchain,
    /// `flake.lock` の根の input。 `flake.nix` が引く nixpkgs の個々の package は読まない。
    Nix,
    /// `.gitmodules`。 名前は path の最後の要素。
    Submodule,
}

impl Ecosystem {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Ecosystem::Cargo => "cargo",
            Ecosystem::Bun => "bun",
            Ecosystem::RustToolchain => "rust-toolchain",
            Ecosystem::Nix => "nix",
            Ecosystem::Submodule => "submodule",
        }
    }
}

/// package の同一性。 `name` は最初に見た綴りで持つ（Cargo は `-` と `_` を同じ名前として扱う）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackageRef {
    pub(crate) ecosystem: Ecosystem,
    pub(crate) name: String,
}

/// 宣言がどの欄にあったか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Section {
    Dependencies,
    DevDependencies,
    BuildDependencies,
    /// Cargo の `[workspace.dependencies]`。 メンバーはここを `workspace = true` で引く。
    WorkspaceDependencies,
    /// npm の peer / optional。
    OtherDependencies,
    Toolchain,
    FlakeInput,
    Submodule,
}

/// manifest の1箇所の宣言。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Declaration {
    /// repo の根からの相対パス。
    pub(crate) manifest: String,
    pub(crate) section: Section,
    /// 要求の書き方（`"1.0"`）。 `workspace = true` は `workspace`、書いていなければ `None`。
    pub(crate) requirement: Option<String>,
}

/// repository から得た package。 複数の manifest・lock をまたいで1件にまとめる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Package {
    pub(crate) package: PackageRef,
    pub(crate) declarations: Vec<Declaration>,
    /// lock が解決した版。 重複なし、昇順。 submodule は空。
    pub(crate) locked_versions: Vec<String>,
}

impl Package {
    /// manifest が宣言している（直接の依存）。 lock にしか無いものは推移的な依存。
    pub(crate) fn is_declared(&self) -> bool {
        !self.declarations.is_empty()
    }
}

/// 読めなかった理由。 パスは別の欄で持つので、ここには値を差し込まない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum IssueReason {
    /// 無い・UTF-8 でない・I/O の失敗。
    Unreadable,
    /// TOML・JSON として読めない。
    Malformed,
}

/// 読み飛ばした manifest。 `walk` が見つけたのに読めなかった。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ManifestIssue {
    pub(crate) path: String,
    pub(crate) reason: IssueReason,
}

/// 読んだ結果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct Manifests {
    /// 名前順。
    pub(crate) packages: Vec<Package>,
    /// 読んだファイル。
    pub(crate) sources: Vec<String>,
    pub(crate) issues: Vec<ManifestIssue>,
}

impl Manifests {
    /// `repo` の版にある manifest と lock をすべて読む。
    ///
    /// 走査は `RepoView::walk` が決める。 `vendor` や `node_modules` の中の manifest は
    /// 読まない（そこは正本ではない）。
    pub(crate) fn read(repo: &RepoView) -> Self {
        let mut builder = Builder::default();
        for path in repo.walk("") {
            let file = path.rsplit('/').next().unwrap_or(path.as_str());
            let kind = match file {
                "Cargo.toml" => SourceKind::CargoManifest,
                "Cargo.lock" => SourceKind::CargoLock,
                "package.json" => SourceKind::PackageJson,
                "bun.lock" => SourceKind::BunLock,
                "rust-toolchain.toml" => SourceKind::Toolchain,
                "flake.lock" => SourceKind::FlakeLock,
                ".gitmodules" => SourceKind::Gitmodules,
                _ => continue,
            };
            let Some(text) = builder.load(repo, &path) else {
                continue;
            };
            let ok = match kind {
                SourceKind::CargoManifest => builder.cargo_manifest(&path, &text),
                SourceKind::CargoLock => builder.cargo_lock(&text),
                SourceKind::PackageJson => builder.package_json(&path, &text),
                SourceKind::BunLock => builder.bun_lock(&text),
                SourceKind::Toolchain => builder.toolchain(&path, &text),
                SourceKind::FlakeLock => builder.flake_lock(&path, &text),
                SourceKind::Gitmodules => {
                    builder.gitmodules(&path, &text);
                    true
                }
            };
            if ok {
                builder.sources.push(path);
            } else {
                builder.issue(path, IssueReason::Malformed);
            }
        }
        builder.finish()
    }
}

#[derive(Clone, Copy)]
enum SourceKind {
    CargoManifest,
    CargoLock,
    PackageJson,
    BunLock,
    Toolchain,
    FlakeLock,
    Gitmodules,
}

/// 名前の照合に使う形。 小文字にして `_` を `-` に寄せる。
pub(super) fn normalize(name: &str) -> String {
    name.to_lowercase().replace('_', "-")
}

#[derive(Default)]
struct Builder {
    packages: BTreeMap<(Ecosystem, String), Package>,
    /// Cargo の、この repo の中の crate（メンバーと path 依存）。 外から取り込んだ部品ではない。
    internal_cargo: BTreeSet<String>,
    sources: Vec<String>,
    issues: Vec<ManifestIssue>,
}

impl Builder {
    fn issue(&mut self, path: String, reason: IssueReason) {
        self.issues.push(ManifestIssue { path, reason });
    }

    fn load(&mut self, repo: &RepoView, path: &str) -> Option<String> {
        // `ViewError` は `repo` が再輸出していないので、型を名指して理由を分けられない。
        // UTF-8 でないものも、読めなかったものに含める。
        if let Ok(Some(text)) = repo.read(path) {
            Some(text)
        } else {
            self.issue(path.to_owned(), IssueReason::Unreadable);
            None
        }
    }

    fn entry(&mut self, ecosystem: Ecosystem, name: &str) -> &mut Package {
        self.packages
            .entry((ecosystem, normalize(name)))
            .or_insert_with(|| Package {
                package: PackageRef {
                    ecosystem,
                    name: name.to_owned(),
                },
                declarations: Vec::new(),
                locked_versions: Vec::new(),
            })
    }

    fn declare(
        &mut self,
        ecosystem: Ecosystem,
        name: &str,
        manifest: &str,
        section: Section,
        requirement: Option<String>,
    ) {
        let declaration = Declaration {
            manifest: manifest.to_owned(),
            section,
            requirement,
        };
        let package = self.entry(ecosystem, name);
        if !package.declarations.contains(&declaration) {
            package.declarations.push(declaration);
        }
    }

    fn lock(&mut self, ecosystem: Ecosystem, name: &str, version: &str) {
        let package = self.entry(ecosystem, name);
        if !package.locked_versions.iter().any(|v| v == version) {
            package.locked_versions.push(version.to_owned());
        }
    }

    fn cargo_manifest(&mut self, path: &str, text: &str) -> bool {
        let Ok(table) = text.parse::<toml::Table>() else {
            return false;
        };
        if let Some(name) = table
            .get("package")
            .and_then(toml::Value::as_table)
            .and_then(|p| p.get("name"))
            .and_then(toml::Value::as_str)
        {
            self.internal_cargo.insert(normalize(name));
        }
        let sections = [
            ("dependencies", Section::Dependencies),
            ("dev-dependencies", Section::DevDependencies),
            ("build-dependencies", Section::BuildDependencies),
        ];
        for (key, section) in sections {
            self.cargo_dependencies(path, table.get(key), section);
        }
        if let Some(workspace) = table.get("workspace").and_then(toml::Value::as_table) {
            self.cargo_dependencies(
                path,
                workspace.get("dependencies"),
                Section::WorkspaceDependencies,
            );
        }
        // `[target.'cfg(...)'.dependencies]`
        if let Some(targets) = table.get("target").and_then(toml::Value::as_table) {
            for target in targets.values().filter_map(toml::Value::as_table) {
                for (key, section) in sections {
                    self.cargo_dependencies(path, target.get(key), section);
                }
            }
        }
        true
    }

    fn cargo_dependencies(&mut self, path: &str, deps: Option<&toml::Value>, section: Section) {
        let Some(deps) = deps.and_then(toml::Value::as_table) else {
            return;
        };
        for (key, value) in deps {
            let mut name = key.as_str();
            let mut requirement = None;
            match value {
                toml::Value::String(version) => requirement = Some(version.clone()),
                toml::Value::Table(spec) => {
                    if let Some(real) = spec.get("package").and_then(toml::Value::as_str) {
                        name = real;
                    }
                    if spec.contains_key("path") {
                        self.internal_cargo.insert(normalize(name));
                    }
                    requirement = spec
                        .get("version")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned)
                        .or_else(|| {
                            (spec.get("workspace").and_then(toml::Value::as_bool) == Some(true))
                                .then(|| "workspace".to_owned())
                        });
                }
                _ => {}
            }
            self.declare(Ecosystem::Cargo, name, path, section, requirement);
        }
    }

    /// `source` を持つ package だけ。 持たないものは、この repo の中の crate。
    fn cargo_lock(&mut self, text: &str) -> bool {
        let Ok(table) = text.parse::<toml::Table>() else {
            return false;
        };
        let Some(packages) = table.get("package").and_then(toml::Value::as_array) else {
            return true;
        };
        for package in packages.iter().filter_map(toml::Value::as_table) {
            let (Some(name), Some(version)) = (
                package.get("name").and_then(toml::Value::as_str),
                package.get("version").and_then(toml::Value::as_str),
            ) else {
                continue;
            };
            if package.contains_key("source") {
                self.lock(Ecosystem::Cargo, name, version);
            }
        }
        true
    }

    fn package_json(&mut self, path: &str, text: &str) -> bool {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            return false;
        };
        let sections = [
            ("dependencies", Section::Dependencies),
            ("devDependencies", Section::DevDependencies),
            ("peerDependencies", Section::OtherDependencies),
            ("optionalDependencies", Section::OtherDependencies),
        ];
        for (key, section) in sections {
            let Some(deps) = json.get(key).and_then(serde_json::Value::as_object) else {
                continue;
            };
            for (name, requirement) in deps {
                let requirement = requirement.as_str();
                // この repo の中の package（workspace・file・link）は、取り込んだ部品ではない。
                if requirement.is_some_and(|r| {
                    ["workspace:", "file:", "link:"]
                        .iter()
                        .any(|p| r.starts_with(p))
                }) {
                    continue;
                }
                self.declare(
                    Ecosystem::Bun,
                    name,
                    path,
                    section,
                    requirement.map(str::to_owned),
                );
            }
        }
        true
    }

    /// `bun.lock` は末尾のカンマを許す JSON（JSONC）。 カンマを除いてから読む。
    fn bun_lock(&mut self, text: &str) -> bool {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(&strip_trailing_commas(text))
        else {
            return false;
        };
        let Some(packages) = json.get("packages").and_then(serde_json::Value::as_object) else {
            return true;
        };
        for entry in packages.values() {
            // `["name@version", registry, { deps }, integrity]`。 先頭だけが要る。
            let Some((name, version)) = entry
                .get(0)
                .and_then(serde_json::Value::as_str)
                .and_then(|s| s.rsplit_once('@'))
            else {
                continue;
            };
            if !name.is_empty() {
                self.lock(Ecosystem::Bun, name, version);
            }
        }
        true
    }

    fn toolchain(&mut self, path: &str, text: &str) -> bool {
        let Ok(table) = text.parse::<toml::Table>() else {
            return false;
        };
        let channel = table
            .get("toolchain")
            .and_then(toml::Value::as_table)
            .and_then(|t| t.get("channel"))
            .and_then(toml::Value::as_str);
        if let Some(channel) = channel {
            self.declare(
                Ecosystem::RustToolchain,
                "rust",
                path,
                Section::Toolchain,
                Some(channel.to_owned()),
            );
            self.lock(Ecosystem::RustToolchain, "rust", channel);
        }
        true
    }

    fn flake_lock(&mut self, path: &str, text: &str) -> bool {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
            return false;
        };
        let Some(nodes) = json.get("nodes").and_then(serde_json::Value::as_object) else {
            return true;
        };
        let root = json
            .get("root")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("root");
        let Some(inputs) = nodes
            .get(root)
            .and_then(|n| n.get("inputs"))
            .and_then(serde_json::Value::as_object)
        else {
            return true;
        };
        for (name, node) in inputs {
            // 配列は他の input を指す `follows`。 それ自身の版は持たない。
            let Some(node) = node.as_str().and_then(|n| nodes.get(n)) else {
                continue;
            };
            let reference = node
                .get("original")
                .and_then(|o| o.get("ref"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned);
            self.declare(Ecosystem::Nix, name, path, Section::FlakeInput, reference);
            if let Some(rev) = node
                .get("locked")
                .and_then(|l| l.get("rev"))
                .and_then(serde_json::Value::as_str)
            {
                self.lock(Ecosystem::Nix, name, rev);
            }
        }
        true
    }

    fn gitmodules(&mut self, path: &str, text: &str) {
        let mut current: Option<(Option<String>, Option<String>)> = None;
        let mut found = Vec::new();
        for line in text.lines().map(str::trim) {
            if line.starts_with('[') {
                found.extend(current.take());
                current = line.starts_with("[submodule").then_some((None, None));
            } else if let Some((key, value)) = line.split_once('=')
                && let Some((p, u)) = current.as_mut()
            {
                match key.trim() {
                    "path" => *p = Some(value.trim().to_owned()),
                    "url" => *u = Some(value.trim().to_owned()),
                    _ => {}
                }
            }
        }
        found.extend(current);
        for (sub_path, url) in found {
            let Some(sub_path) = sub_path else { continue };
            let name = sub_path.rsplit('/').next().unwrap_or(sub_path.as_str());
            self.declare(Ecosystem::Submodule, name, path, Section::Submodule, url);
        }
    }

    fn finish(self) -> Manifests {
        let Builder {
            packages,
            internal_cargo,
            sources,
            issues,
        } = self;
        let mut packages: Vec<Package> = packages
            .into_iter()
            .filter(|((ecosystem, name), _)| {
                !(*ecosystem == Ecosystem::Cargo && internal_cargo.contains(name))
            })
            .map(|(_, mut package)| {
                package.declarations.sort();
                package.locked_versions.sort();
                package
            })
            .collect();
        packages.sort_by(|a, b| a.package.cmp(&b.package));
        let mut sources = sources;
        sources.sort();
        let mut issues = issues;
        issues.sort();
        Manifests {
            packages,
            sources,
            issues,
        }
    }
}

/// JSONC の、`}` か `]` の直前にあるカンマを除く。 文字列の中は触らない。
pub(super) fn strip_trailing_commas(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut in_string = false;
    let mut escaped = false;
    for (i, &c) in chars.iter().enumerate() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => {
                in_string = true;
                out.push(c);
            }
            ',' => {
                let next = chars[i + 1..].iter().find(|n| !n.is_whitespace());
                if !matches!(next, Some('}' | ']')) {
                    out.push(c);
                }
            }
            _ => out.push(c),
        }
    }
    out
}
