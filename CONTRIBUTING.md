# KOERU への貢献

Issue と Pull Request を歓迎します。

まず [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) を読んでください。設計の前提は [docs/product-vision.md](docs/product-vision.md) にあります。確定している方針に反する変更は、その方針を変えるべき理由から議論してください。 実装の詳細から入ると噛み合いません。

## 最初に git-lfs と submodule を用意してください

**先に `git-lfs` を入れてください。** MFA の音響モデルは HuggingFace のリポジトリを
submodule にしていて（`meta/decisions/DEC-ALN-012.toml`）、実体が LFS に入っています。
入れずに clone すると `git-lfs filter-process: command not found` で途中で死にます。

```bash
brew install git-lfs   # macOS。他は https://git-lfs.com
git lfs install
```

C / C++ の依存（WORLD と Kaldi）も submodule です（`meta/decisions/DEC-PLT-016.toml`）。

```bash
git clone --recurse-submodules https://github.com/dino3616/koeru
# もう clone してしまった場合
git submodule update --init --recursive
```

取っていないと `build.rs` が止まります。C++ のコンパイルエラーではなく、この手順を出して落ちるようにしてあります。

モデルが無くてもアプリは動きます。 自動原音設定が音響モデルを使わない退避経路に落ちるだけで、
録音も試唱も止まりません（ログに「自動原音設定は退避経路で動く」と出ます）。

## DCO — すべてのコミットに Signed-off-by が必要です

このプロジェクトは [Developer Certificate of Origin](https://developercertificate.org/) を採用しています。CLA はありません（再ライセンスの予定がないため）。

```bash
git commit -s -m "..."
```

`-s` を付けると `Signed-off-by: Your Name <your@email.example>` が追加されます。これは「自分にはこのコードを AGPL-3.0-or-later で提供する権利がある」という表明です。 忘れた場合は `git commit --amend -s` か `git rebase --signoff` で追加してください。CI が全コミットを検査します。

## ライセンスの制約

KOERU は AGPL-3.0-or-later です。ここから2つの制約が出ます。

依存を追加するとき、AGPL-3.0-or-later に取り込めるライセンスでなければ CI が落ちます。 許可リストは [`deny.toml`](deny.toml) にあり、`cargo deny check` が機械判定します。非商用限定（CC BY-NC 系）、再配布禁止、独自条項のものは通りません。 許可リストに追加が必要な場合は、PR の説明にライセンス種別と一次情報の URL を書いてください。

学習済みモデルやデータセットを追加するときは、より慎重に見ます。 モデル側の表示ライセンスが、学習に使われたコーパスの条件を上書きできるとは限りません。「モデルに CC BY と書いてあるから大丈夫」は根拠になりません。 学習データの出所とそれぞれの条件を示してください。この理由で候補をいくつか落としています（`meta/decisions/DEC-ALL-001.toml` 参照）。

## コードの規約

規約は `.agents/skills/` にあります。 作業のときに読み込まれる Agent Skill として管理していますが、人間が読んでも同じものです。

| | 何が書いてあるか |
|---|---|
| [`writing-comments`](.agents/skills/writing-comments/SKILL.md) | コメントの書き方。言語を問わない |
| [`rust-conventions`](.agents/skills/rust-conventions/SKILL.md) | エラー型、トレース、lint、依存追加 |
| [`react-conventions`](.agents/skills/react-conventions/SKILL.md) | 画面。tailwind-variants、`className` を受けないこと、Rust との境界 |
| [`verify-koeru`](.agents/skills/verify-koeru/SKILL.md) | 何をどの順に走らせるか |

特に次の3つは PR で必ず見ます。

- ドメイン層で `anyhow::Error` を返さない。 `thiserror` の列挙体を返す。畳むのはアプリケーション境界だけ
- `?` を並べる関数には `#[tracing::instrument(err)]` を付ける
- `println!` / `eprintln!` / `dbg!` を使わない。 出力は `tracing` に統一する（例外は実機ハーネスだけ。下記）

`clippy::all` はリポジトリ全体で deny です。例外は行単位・ブロック単位の `#[allow(...)]` で入れ、理由をコメントに書いてください。

ファイル先頭の `#![allow(...)]` は、実機ハーネス（`tests/guide_leak.rs` など）の `clippy::print_stdout` だけです。 あれは走らせた本人が数値を読む出力で、`tracing` に出すと既定のフィルタで見えません。そこにも理由をコメントで添えています。`dbg!` はテストでも禁止のままです。

## 送信してはいけないもの

利用計測はオプトインで、送信フィールドはホワイトリスト方式です。送ってよいフィールド名を列挙し、そこに無いものは通しません。

**音源名・ファイルパス・歌詞・プロジェクト名・波形をトレースに載せないでください。** 音源名はキャラクター名でもあり、本人の創作物です。KOERU は「非公開のまま完成できる」ことを担保しているので、トレースがそれを漏らすと製品の前提が崩れます。エラーを送るときは種別を表す固定文字列（`"recording.disk_full"` のような）だけを使い、`Display` を送らないでください。

## 手元での確認

```bash
# Rust
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check   # 要 cargo install cargo-deny --locked

# 要件・判断・予算の登録簿と、ID 参照の解決
cargo xtask check-meta && cargo xtask check-budgets
cargo xtask check-coverage && cargo xtask check-references

# 画面
cd crates/koeru-app/ui && bun install && bun run check:ci && bun run build
```

CI と同じものです。

音声のバックエンドは macOS しかありません。 他の OS では `backend/unsupported.rs` が
選ばれるので、そちらでも組み立つことを手元で通してください。見ないと、CI で初めて気づきます。

```bash
F='--cfg koeru_force_unsupported_backend'
RUSTFLAGS="$F" cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTFLAGS="$F" RUSTDOCFLAGS="$F" cargo test --workspace --all-features
```

`RUSTDOCFLAGS` を忘れないでください。 `RUSTFLAGS` は rustdoc に届かないので、
doctest だけが違う設定でコンパイルされます。

画面へ渡す型は Rust から生成しています（`meta/decisions/DEC-PLT-019.toml`）。
コマンドや境界の型を足したら作り直してください。`bindings.gen.ts` は手で直しません。

```bash
KOERU_WRITE_BINDINGS=1 cargo test -p koeru-app --test bindings
```

## エージェントが作った PR について

クラッシュレポートから自動起票された Issue に対して、エージェントが修正 PR を出すことがあります。人間の PR と同じ基準でレビューします。 加えて次の点を明示してください。

- エージェントが生成した PR であることを説明に書く
- 音声処理・DSP・ライセンスに関わる変更は、根拠を人間が確認したことを書く。 この3領域はエージェントが誤りやすく、CI では捕まらない誤りが入ります

## PR の粒度

1つの PR で1つのことをしてください。lint の修正と機能追加を混ぜないでください。設計方針に触る変更は、先に Issue で合意してから送ってください。実装を捨てさせるのは双方にとって損です。
