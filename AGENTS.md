# AGENTS.md

このリポジトリでエージェントが作業するときの前提。**人間向けの入口は [README.md](README.md)、貢献の手順は [CONTRIBUTING.md](CONTRIBUTING.md)。**

## このリポジトリは何か

**KOERU** — UTAU 向けの歌声ライブラリ制作スタジオ。録音から配布パッケージ生成までを一つのプロジェクトで扱い、**録音の途中でも自分の声で歌を聴けること**を中核に置く。

**設計段階で、動くものはまだない。** 設計の決定はすべて `docs/` にある。**実装より先に、触る領域の文書を読むこと。**

| 文書 | 何が書いてあるか |
|---|---|
| [docs/product-vision.md](docs/product-vision.md) | **確定している方針。ここに反することはしない** |
| [docs/personas.md](docs/personas.md) | 誰のために作るか。ミナ / ハル / ソラ |
| [docs/journey-map.md](docs/journey-map.md) | 体験の時系列（As-Is / To-Be） |
| [docs/usecase-map.md](docs/usecase-map.md) | 機能と利用関係。F01〜F05、UC-* |
| [docs/tech-requirements.md](docs/tech-requirements.md) | 何を満たさないと成立しないか。TR-* と未解決の設計課題 |

## Skills

**正本は `.agents/skills/`。`.claude/skills/` は symlink。** 追加・編集は必ず `.agents/skills/` 側で行い、`.claude/skills/` には symlink を張るだけにする。実体を両方に置かない。

| Skill | いつ使うか |
|---|---|
| `rust-conventions` | **Rust のコードを書く・直す・レビューするとき。** エラー型、tracing、clippy、依存追加の方針 |

## 破ってはいけないもの

**1. ドメイン層で `anyhow::Error` を返さない。** `thiserror` の列挙体を返す。畳むのはアプリケーション境界だけ。詳細は `rust-conventions` skill。

**2. `println!` / `eprintln!` / `dbg!` を使わない。** 出力は `tracing` に統一する。lint で deny されている。

**3. トレースに音源名・ファイルパス・歌詞・プロジェクト名を載せない。** 送信フィールドはホワイトリスト方式にする。送ってよいフィールド名を列挙した定数を1箇所に置き、そこに無いものは通さない。**KOERU は「非公開のまま完成できる」ことを担保している製品なので、これが漏れると前提が崩れる。**

**4. 依存を追加するときはライセンスを確認する。** AGPL-3.0-or-later に取り込めるものだけ。許可リストは `deny.toml`、判定は `cargo deny check`。**学習済みモデルは、モデル側の表示ライセンスが学習コーパスの条件を上書きできるとは限らない。**

**5. コミットには `Signed-off-by` を付ける。** `git commit -s`。DCO を採用している。CI が全コミットを検査する。

**6. `docs/` に日付を書かない。** これらは継続的に参照するリファレンスで、決定ログではない。「2026年時点」「現在」のような時点表現も避け、常に現在を語る文書として書く。**方針が変わったら該当行を書き換える。履歴は残さない。**

## 検証

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check   # 要 cargo install cargo-deny --locked
```

CI（`.github/workflows/ci.yml`）で同じものを実行する。`clippy::all` はリポジトリ全体で deny。

## リポジトリの構成

```
.agents/skills/     Skills の正本
.claude/skills/     .agents/skills への symlink
crates/koeru-core/  ドメイン層。GUI と OS に依存しない（実装は未着手）
docs/               設計文書
Cargo.toml          [workspace.lints] で clippy::all を deny
deny.toml           AGPL 互換ライセンスの許可リスト
```

## 実装で押さえておくこと

- **実装は Rust + Tauri。** 単一のネイティブアプリ、PC 前提。処理はローカル完結で、声をサーバへ送らない
- **音声 I/O は miniaudio を FFI で使う。** 必要なのは OS 側の音声加工を無効化する経路（排他モード、または共有モード＋ RAW ストリーム要求）へ到達できることで、`cpal` はそのどちらにも降りられない
- **合成は WORLD ベース。** ニューラルボコーダへの置き換えは採らない（「あなたの声そのもの」が「生成された声」に変わるため）
- **未解決の設計課題が2件ある**（`docs/tech-requirements.md` の B5 / B6）。いずれもメモリ予算の破綻で、実装着手前に数値を積み直す必要がある

## 注意

`.claude/skills/` の symlink は、Windows で `core.symlinks` が無効だとパス文字列のプレーンファイルとして展開される。その場合は `.agents/skills/` 側を直接参照すること。
