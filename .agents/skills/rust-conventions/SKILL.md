---
name: rust-conventions
description: KOERU の Rust コードを書く・直す・レビューするときに必ず読む規約。エラー型の設計（thiserror と anyhow の層別使い分け）、tracing の入れ方と送信してよいフィールド、clippy の方針と例外の入れ方、検証コマンドを定める。Rust ファイルの追加・編集、エラーハンドリングの実装、ログやトレースの追加、clippy 違反の修正、crate の追加、FFI の実装、PR レビューのときに使う。
---

# KOERU — Rust の規約

エラーハンドリング・トレース・lint の方針。**コードの書き方の好みではなく、破ると後から回復しにくいものだけを書く。**

プロダクトの前提は [docs/product-vision.md](../../../docs/product-vision.md)、技術要件は [docs/tech-requirements.md](../../../docs/tech-requirements.md)。

## 前提として置いている性質

KOERU は**回復が必要な失敗を多く抱える**アプリケーションである。録音デバイスが消える、ディスクが埋まる、モデルが読めない、アライメントの確信度が低い、必要なサンプルが未収録、CP932 で表現できない文字が来る。**いずれも「落ちてよい」失敗ではなく、呼び出し側が分岐して対処すべき失敗**である。

加えて、**録音は「やり直しが高い操作」**である。3時間の収録の途中で失敗したとき、何が起きたかを後から追えないと、利用者はその日の作業を失う。したがって**追跡性は機能要件に近い。**

## エラー型の設計

### 層ごとの責務

| 層 | 何を使うか | 理由 |
|---|---|---|
| **ブートストラップ層**（`main`、トレース初期化、設定読み込み） | `.expect("...")` | 回復する意味がなく、失敗したら即座に気づけるほうが良い |
| **ドメイン層**（`koeru-core` 以下） | **`thiserror` の列挙体** | 呼び出し側が `match` で網羅的に分岐できる |
| **アプリケーション境界**（Tauri コマンド、CLI） | **`anyhow::Error`** に畳んでよい | ここより先に回復の余地はない |
| **ロギング** | `tracing` の属性マクロ | 各関数に付けるだけで追跡できる |

### 守ること

- **ドメイン層で `anyhow::Error` を返さない。** 畳んだ時点で網羅性が失われ、呼び出し側は文字列を見る以外の手段を持たなくなる。これが最大のアンチパターン。
- **エラー列挙体に `#[non_exhaustive]` を付けない。** 網羅性チェックを効かせるため。バリアントの追加は破壊的変更として扱う。
- **原因を捨てない。** 下位のエラーは `#[source]` で繋ぐ。`.to_string()` して詰め直さない。
- **`unwrap()` は禁止（lint で deny）。** `expect()` はブートストラップ層でのみ使い、**メッセージに「何を期待していたか」を書く。**
- **失敗ではないものをエラーにしない。** 「アライメントの確信度が低い」は人に確認させるための入力であって失敗ではないので、結果型で表してエディタへ回す。「部分音源で必要なサンプルが無い」も、選んだ方式のカバレッジでは通常起きうる状態なので、エラーにせずカバレッジ判定で事前に弾く。**エラー列挙体に入れてよいのは、呼び出し側が続行を諦める必要があるものだけ。**

### `anyhow` の既知の弱点

`anyhow` は**リリースビルドでエラーの発生位置が分からなくなる。** これがドメイン層で `anyhow` を使わない実務上の理由でもある。アプリケーション境界で使うときは、**必ず `#[tracing::instrument(err)]` と併用して位置を別経路で残す。**

## トレース

### 守ること

- **`?` を並べる関数には `#[tracing::instrument(err)]` を付ける。** 付けないとどこで失敗したのかが追えない。`?` の多用とトレース不在の組み合わせが、原因究明を最も難しくする。
- **`println!` / `eprintln!` / `dbg!` は禁止（lint で deny）。** 出力は `tracing` に統一する。
- **span は工程の単位で切る。** 録音1テイク、アライメント1件、書き出し1回。利用者から見た「操作」と一致させる。
- **`instrument` に載せるフィールドを選ぶ。** 既定では引数が全部記録されるので、パスや歌詞を含む引数は `skip` する。

```rust
#[tracing::instrument(skip(pcm), fields(take_index, method = ?method), err)]
fn finalize_take(pcm: &[f32], take_index: usize, method: Method) -> Result<TakeId> { /* ... */ }
```

### 3つの出力段

1. **`fmt` 層** — 開発時の人間向け。`RUST_LOG` で制御する。
2. **ファイル層** — 利用者の端末にローカル保存する。障害報告に添付できる。
3. **送信層** — **オプトインのときだけ**有効。既定は無効。

### 送信層はホワイトリスト方式にする

**ブラックリスト方式では必ず漏れる。** 送ってよいフィールド名を列挙した定数を1箇所に置き、そこに無いものは通さない。

**送ってはいけないもの**: 音源名（＝キャラクター名。本人の創作物）、ファイルパス（利用者名を含む）、歌詞、プロジェクト名、波形。**非公開のまま完成できることを担保した製品が、トレースで音源名を送っていたら意味がない。**

エラーを送るときは種別を表す固定文字列（`"recording.disk_full"` のような）だけを使い、エラー型にはその文字列を返すメソッドを持たせる。エラーの `Display` を送らない。`Display` にはパスや文字列が入る。

## lint

### 方針

**`clippy::all` はリポジトリ全体で常に deny。** 設定は `Cargo.toml` の `[workspace.lints]` に置き、各クレートは `[lints] workspace = true` で継承する。

各クレートの `lib.rs` に `#![deny(clippy::all)]` を書く方式は採らない。**クレートを追加したときに書き忘れると静かに無効化される**ためで、ワークスペース側に置けば継承が既定になる。CI の `cargo clippy -- -D warnings` は、設定が外れた場合の backstop。

`clippy::all` は `priority = -1` にしてあるので、**個別の lint 指定が常に優先される。**

**新しいクレートを追加したら、必ず `[lints] workspace = true` を書く。**

### 追加している lint

| lint | 水準 | 理由 |
|---|---|---|
| `clippy::unwrap_used` | deny | 回復可能な失敗を握り潰さない |
| `clippy::expect_used` | allow | ブートストラップ層で使うため |
| `clippy::print_stdout` / `print_stderr` / `dbg_macro` | deny | 出力は `tracing` に統一する |
| `clippy::todo` / `unimplemented` | warn | 実装中に手を止めない |
| `rust::unsafe_op_in_unsafe_fn` | deny | FFI（miniaudio、WORLD）で `unsafe` は避けられないので、範囲を明示させる |

`unsafe_code` そのものは禁止しない。miniaudio と WORLD への FFI が必須のため。代わりに `unsafe` ブロックには `// SAFETY:` コメントを必ず書く（`clippy::missing_safety_doc` が `clippy::all` に含まれる）。

### 例外の入れ方

**行単位・ブロック単位の `#[allow(...)]` で入れる。理由をコメントで残す。**

```rust
// FFI の戻り値は C 側で範囲が保証されている。
#[allow(clippy::cast_possible_truncation)]
let frames = raw_frames as usize;
```

**クレート全体の `#![allow(...)]` は使わない。** 範囲が広すぎて、後から入った本物の問題を隠す。

テストコードでの緩和は `clippy.toml` 側で設定済み（`allow-unwrap-in-tests`、`allow-expect-in-tests`、`allow-panic-in-tests`）。**`dbg!` はテストでも禁止のままにしてある**（消し忘れがそのまま入るため）。

## 依存を追加するとき

**KOERU は AGPL-3.0-or-later。** 許可リストは `deny.toml` にあり、`cargo deny check` が機械判定する。**非商用限定（CC BY-NC 系）、再配布禁止、独自条項のものは通らない。**

**学習済みモデルやデータセットはより慎重に見る。** モデル側の表示ライセンスが、学習に使われたコーパスの条件を上書きできるとは限らない。**「モデルに CC BY と書いてあるから大丈夫」は根拠にならない。** この判断を誤って一度候補を落としている。

## 検証

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

CI で同じものを実行する。
