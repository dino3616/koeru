# KOERU

**声を、誰もが手渡せる創作物にする。**

KOERU は UTAU 向けの歌声ライブラリ制作スタジオです。録音から配布パッケージの生成までを一つのプロジェクトで扱い、**録音の途中でも自分の声で歌を聴けること**を中核に置いています。

「現代版 OREMO」ではありません。中心にあるのは「録音リストと oto.ini を操作する」ではなく、**声を録ると、途中でも自分の声が歌い始める**という体験です。

## 3つの原則

- **Create before configure** — 設定する前に、まず歌える
- **Own your voice** — 自分の声は自分のもの
- **Made to be shared** — 誰かに使われて、初めて完成する

## 状態

**設計段階です。動くものはまだありません。** 設計文書が `docs/` にあります。

| 文書 | 内容 |
|---|---|
| [product-vision.md](docs/product-vision.md) | なぜ・誰に・何を作るか。確定している方針 |
| [personas.md](docs/personas.md) | 誰のために作るか |
| [journey-map.md](docs/journey-map.md) | 体験の時系列（現状と理想） |
| [usecase-map.md](docs/usecase-map.md) | 機能と利用関係 |
| [tech-requirements.md](docs/tech-requirements.md) | 何を満たさないと成立しないか |
| [rust-conventions.md](docs/rust-conventions.md) | エラーハンドリング・トレース・lint の方針 |

## 実装の方針

- **単一のネイティブアプリケーション**（Rust + Tauri）。PC 前提
- **処理はローカルで完結する。声をサーバへ送らない**
- **オフラインで全機能が動く**
- **まず UTAU エコシステムをサポートする**
- 原音設定は自動を既定とするが、**setParam / vLabeler と同等の編集機能を必ず持つ**

## 開発

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

`clippy::all` はリポジトリ全体で deny です。詳細は [rust-conventions.md](docs/rust-conventions.md)。

## 貢献

Issue と Pull Request を歓迎します。[CONTRIBUTING.md](CONTRIBUTING.md) を読んでください。**すべてのコミットに DCO の `Signed-off-by` が必要です。**

## ライセンス

[AGPL-3.0-or-later](LICENSE)

派生物もソースが公開され続けることを意図しています。「声は本人が所有し、持ち運び、渡せる創作資産である」という思想と、ソフトウェア自体が開かれ続けることを揃えるためです。
