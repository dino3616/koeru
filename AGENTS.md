# AGENTS.md

このリポジトリでエージェントが作業するときの前提。**人間向けの入口は [README.md](README.md)、貢献の手順は [CONTRIBUTING.md](CONTRIBUTING.md)。**

## このリポジトリは何か

**KOERU** — UTAU 向けの歌声ライブラリ制作スタジオ。録音から配布パッケージ生成までを一つのプロジェクトで扱い、**録音の途中でも自分の声で歌を聴けること**を中核に置く。

**設計段階で、動くものはまだない。** **実装より先に、触る領域の文書を読むこと。**

正本は3面に分かれている。**どれが何を持つかを取り違えないこと。**

| 面 | 何の正本か | 場所 |
|---|---|---|
| **FSL** | 形式的な契約。状態・遷移・不変条件・受入・禁止 | `specs/` |
| **Meta** | 決定・未決の論点・調査 Evidence・実測予算・リリース対象 | `meta/` |
| **Markdown** | 背景・物語・意図・調査の説明 | `docs/` |

| 文書 | 何が書いてあるか |
|---|---|
| [docs/product-vision.md](docs/product-vision.md) | **確定している方針。ここに反することはしない** |
| [docs/personas.md](docs/personas.md) | 誰のために作るか。ミナ / ハル / ソラ |
| [docs/journey-map.md](docs/journey-map.md) | 体験の時系列（As-Is / To-Be） |
| [docs/usecase-map.md](docs/usecase-map.md) | 機能と利用関係。F01〜F05、UC-* |
| [docs/generated/](docs/generated/) | **FSL から生成した要件文書。手で編集しない** |

技術的なことは `docs/` には無い。**要件・判断・未決の論点・予算は [meta/](meta/)、形式的な契約は [specs/](specs/) が正本。**

| 場所 | 何の正本か |
|---|---|
| [meta/requirements/](meta/requirements/) | 満たさなければ成立しない条件。`TR-*` 261件 |
| [meta/decisions/](meta/decisions/) | 何を選び、なぜ選び、何が起きたら覆すか。`DEC-*` |
| [meta/questions/](meta/questions/) | まだ決まっていないこと。`Q-*` |
| [meta/budgets/](meta/budgets/) ・ [meta/targets/](meta/targets/) | 上限と配分、領域ごとの性能目標 |
| [meta/technologies/](meta/technologies/) | 候補技術とライセンス、採否 |
| [meta/evidence/](meta/evidence/) | 判断の根拠。調査資料、ライセンス、実測 |
| [specs/](specs/) | 状態・遷移・不変条件・受入・禁止。反例探索にかけている |

読み方と規律は [meta/README.md](meta/README.md) と [specs/README.md](specs/README.md)。

## Skills

**正本は `.agents/skills/`。`.claude/skills/` は symlink。** 追加・編集は必ず `.agents/skills/` 側で行い、`.claude/skills/` には symlink を張るだけにする。実体を両方に置かない。

| Skill | いつ使うか |
|---|---|
| `rust-conventions` | **Rust のコードを書く・直す・レビューするとき。** エラー型、tracing、clippy、依存追加の方針 |
| `fsl` | **`specs/` の FSL を書く・直すとき。** 言語仕様、検証器、反例からの修復手順 |
| `fsl-requirements` / `fsl-design` | 要求層 / 設計層を自然言語から起こすとき |

**FSL を書く前に、形式化メモをチャットに出して確認を取ること。** 出典に無い要件を推測で埋めない。`fslc` が保証するのは「書かれたモデルの内部整合性」であって、モデルが KOERU の意図を正しく表しているかは人が確かめる。

## 破ってはいけないもの

**1. ドメイン層で `anyhow::Error` を返さない。** `thiserror` の列挙体を返す。畳むのはアプリケーション境界だけ。詳細は `rust-conventions` skill。

**2. `println!` / `eprintln!` / `dbg!` を使わない。** 出力は `tracing` に統一する。lint で deny されている。

**3. トレースに音源名・ファイルパス・歌詞・プロジェクト名を載せない。** 送信フィールドはホワイトリスト方式にする。送ってよいフィールド名を列挙した定数を1箇所に置き、そこに無いものは通さない。**KOERU は「非公開のまま完成できる」ことを担保している製品なので、これが漏れると前提が崩れる。**

**4. 依存を追加するときはライセンスを確認する。** AGPL-3.0-or-later に取り込めるものだけ。許可リストは `deny.toml`、判定は `cargo deny check`。**学習済みモデルは、モデル側の表示ライセンスが学習コーパスの条件を上書きできるとは限らない。**

**5. コミットには `Signed-off-by` を付ける。** `git commit -s`。DCO を採用している。CI が全コミットを検査する。

**6. FSL と meta が所有する命題を、手書き文書で再定義しない。** 同じ規則を2箇所に書くと、片方だけが変わる。手書き文書には ID で参照させる。

> 悪い例: `F0 推定は SwiftF0 を採用する。`
> 良い例: `F0 推定の方針は DEC-SYN-001 を参照。`

**7. 生成文書を手で直さない。** `docs/generated/` は FSL から決定論的に生成している。**Markdown から FSL への逆同期はしない。** 直すのは常に FSL 側で、文書は再生成する。CI が drift を検出して落とす。`background` スロットの中だけは自由に書ける。

**8. `docs/` に日付を書かない。** これらは継続的に参照するリファレンスで、決定ログではない。「2026年時点」「現在」のような時点表現も避け、常に現在を語る文書として書く。**方針が変わったら該当行を書き換える。履歴は残さない。**

## 検証

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check   # 要 cargo install cargo-deny --locked
```

仕様側は別に検証する。

```bash
fslc lint specs/ --project specs/fsl-project.toml   # ID 規約
fslc chain specs/fsl-project.toml                   # 各層の検証と、層の継ぎ目の refine
fslc document check specs/requirements/project-lifecycle.fsl docs/generated/project-lifecycle.md
cargo xtask check-meta                                 # meta の参照先が実在するか
cargo xtask check-budgets                              # 配分の合計が上限を超えていないか
cargo xtask check-profile <ID>                         # 未決の論点がリリースを塞いでいないか
```

CI（`.github/workflows/ci.yml`）で同じものを実行する。`clippy::all` はリポジトリ全体で deny。

**`check-profile` は通常の CI では走らせない。** 未決が残っているのは開発中は正常で、その状態でリリースするのが異常だという線引きにしている。実行は `.github/workflows/release-gate.yml`（タグ push と手動）。

仕様を書き換えたら、変異検査で空洞になっていないかを見ること。生き残りは失敗ではなく、レビュー待ちの列として扱う。

```bash
fslc mutate specs/requirements/project-lifecycle.fsl --depth 8
```

## リポジトリの構成

```
.agents/skills/     Skills の正本
.claude/skills/     .agents/skills への symlink
crates/koeru-core/  ドメイン層。GUI と OS に依存しない（実装は未着手）
xtask/              仕様ゲート。meta と FSL / 技術要件の ID を突き合わせる
specs/              FSL の正本。requirements / design / refinement
meta/               決定・未決の論点・Evidence・予算・リリースプロファイル
docs/               設計文書
docs/generated/     FSL から生成した文書。手で編集しない
Cargo.toml          [workspace.lints] で clippy::all を deny
deny.toml           AGPL 互換ライセンスの許可リスト
```

## 実装で押さえておくこと

- **実装は Rust + Tauri。** 単一のネイティブアプリ、PC 前提。処理はローカル完結で、声をサーバへ送らない
- **音声 I/O は miniaudio を FFI で使う。** 必要なのは OS 側の音声加工を無効化する経路（排他モード、または共有モード＋ RAW ストリーム要求）へ到達できることで、`cpal` はそのどちらにも降りられない
- **合成は WORLD ベース。** ニューラルボコーダへの置き換えは採らない（「あなたの声そのもの」が「生成された声」に変わるため）
- **未解決の設計課題が2件ある**（`meta/questions/Q-PLT-001` と `Q-EDT-001`）。いずれもメモリ予算の破綻で、実装着手前に数値を積み直す必要がある
- **FSL 化してあるのは縦切り1本だけ**（録音 → テイク確定 → 完成 → 非公開のまま終了 → ZIP 書き出し）。261件の技術要件を一度に FSL へ移さないこと。形式化できない文章まで入れると、FSL が新しい巨大文書になる
- **`fslc` はバージョンと SHA-256 で固定している**（CI 参照）。更新は Renovate 任せにせず、semantic diff を確認してから上げる。FSL 内部の crate を直接 import せず、CLI の JSON 出力だけに依存する

## 注意

`.claude/skills/` の symlink は、Windows で `core.symlinks` が無効だとパス文字列のプレーンファイルとして展開される。その場合は `.agents/skills/` 側を直接参照すること。
