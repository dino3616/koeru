# KOERU

声を、誰もが手渡せる創作物にする。

KOERU は UTAU 向けの歌声ライブラリ制作スタジオです。録音から配布パッケージの生成までを一つのプロジェクトで扱い、録音の途中でも自分の声で歌を聴けることを中核に置いています。

「現代版 OREMO」ではありません。中心にあるのは「録音リストと oto.ini を操作する」ではなく、声を録ると、途中でも自分の声が歌い始めるという体験です。

## 3つの原則

- Create before configure — 設定する前に、まず歌える
- Own your voice — 自分の声は自分のもの
- Made to be handed on — 手渡せる状態まで、つくる

## 状態

M2 を実装中です。 録音してテイクを確定し、その場で自分の声に歌わせるところまでが動きます。
配布パッケージの生成（M4）と原音設定エディタ（M6）はこれからです。
マイルストーンの区切りは [meta/profiles/](meta/profiles/) にあります。

使う人向けの手引きはまだありません。 M2 の時点で配布できる成果物が無いので、
導入手順や操作の手引きを書いても対象がありません。下の一覧は背景と方針で、
使い方ではありません。M4（配布パッケージ生成）で書きはじめます。

| 文書 | 内容 |
|---|---|
| [product-vision.md](docs/product-vision.md) | なぜ・誰に・何を作るか。確定している方針 |
| [personas.md](docs/personas.md) | 誰のために作るか |
| [journey-map.md](docs/journey-map.md) | 体験の時系列（現状と理想） |
| [usecase-map.md](docs/usecase-map.md) | 機能と利用関係 |
| [meta/](meta/) | 何を満たさないと成立しないか。要件・判断・未決の論点・予算 |
| [specs/](specs/) | 形式的な契約。反例探索にかけている |
| [AGENTS.md](AGENTS.md) | エージェントが作業するときの前提と、破ってはいけないもの |

## 実装の方針

- 単一のネイティブアプリケーション（Rust + Tauri）。PC 前提
- 処理はローカルで完結する。声をサーバへ送らない
- オフラインで全機能が動く
- まず UTAU エコシステムをサポートする
- 原音設定は自動を既定とするが、setParam / vLabeler と同等の編集機能を必ず持つ

## 開発

先に `git-lfs` を入れてから submodule を取ってください。 WORLD と Kaldi、
MFA の音響モデルを submodule で調達しており、モデルの実体は LFS にあります。
入れずに clone すると途中で止まります。手順は [CONTRIBUTING.md](CONTRIBUTING.md) にあります。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

WebView 側は `crates/koeru-app/ui` で完結します。

```bash
cd crates/koeru-app/ui && bun install && bun run check && bun run build
```

`clippy::all` はリポジトリ全体で deny です。コードの規約は [`.agents/skills/`](.agents/skills/) にあります——コメント、Rust、画面、検証の4つ。作業のときに読み込まれる Agent Skill として管理していますが、人間が読んでも同じものです。

## 貢献

Issue と Pull Request を歓迎します。[CONTRIBUTING.md](CONTRIBUTING.md) を読んでください。すべてのコミットに DCO の `Signed-off-by` が必要です。

## ライセンス

[AGPL-3.0-or-later](LICENSE)

派生物もソースが公開され続けることを意図しています。「声は本人が所有し、持ち運び、渡せる創作資産である」という思想と、ソフトウェア自体が開かれ続けることを揃えるためです。
