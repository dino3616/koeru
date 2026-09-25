---
name: rust-conventions
description: KOERU の Rust コードを書く・直す・レビューするときに必ず読む規約。エラー型の設計（thiserror の列挙体、境界での分類と確定の状態、失敗の記録）、tracing の入れ方と送信してよいフィールド、clippy の方針と例外の入れ方、検証コマンドを定める。Rust ファイルの追加・編集、エラーハンドリングの実装、ログやトレースの追加、clippy 違反の修正、crate の追加、FFI の実装、PR レビューのときに使う。
---

# KOERU — Rust の規約

エラーハンドリング・トレース・lint の方針。コードの書き方の好みではなく、破ると後から回復しにくいものだけを書く。

プロダクトの前提は [docs/product-vision.md](../../../docs/product-vision.md)、技術要件は [meta/requirements/](../../../meta/requirements/)。

## 前提として置いている性質

KOERU は回復が必要な失敗を多く抱えるアプリケーションである。録音デバイスが消える、ディスクが埋まる、モデルが読めない、アライメントの確信度が低い、必要なサンプルが未収録、CP932 で表現できない文字が来る。いずれも「落ちてよい」失敗ではなく、呼び出し側が分岐して対処すべき失敗である。

加えて、録音は「やり直しが高い操作」である。3時間の収録の途中で失敗したとき、何が起きたかを後から追えないと、利用者はその日の作業を失う。したがって追跡性は機能要件に近い。

## エラー型の設計

分類・code・確定の状態・次の手の決め方は `DEC-PLT-038` が持つ。ここにあるのは、それを Rust で書くときの手順だけ。

- 語彙（`Class` / `Outcome` / `Action` と trait `Failure`）は葉の crate `koeru-failure`。エラー型はそれぞれ `impl koeru_failure::Failure` を持ち、変種ごとに `code()` と `class()` を返す。入出力の失敗は `koeru_failure::io_class` で分ける
- 境界は `koeru-app` の `AppError`。下の層は `AppError::from_failure`（`?` で畳めるものは `From`）、境界で初めて分かる失敗は `AppError::new(code, class, 文言)`。確定したあとで起きた失敗は `with_outcome(Outcome::Committed)` で上書きする。**移行中の形**で、GraphQL の payload（`DEC-PLT-035`）に置き換わる
- 画面へ渡す失敗は、直列化のときに1回だけ記録される。 画面へ渡さずにその場で縮退するときは、持ち主が `koeru_failure::record_failure` か `AppError::record` を呼ぶ
- 文言に値を差し込まないこと、code が型をまたいで一意なこと、`instrument` に `err` が無いことは `crates/koeru-app/tests/failures.rs` と `offline.rs` が source を読んで見る

### 層ごとの責務

| 層 | 何を使うか | 理由 |
|---|---|---|
| ブートストラップ層（`main`、トレース初期化、設定読み込み） | `.expect("...")` | 回復する意味がなく、失敗したら即座に気づけるほうが良い |
| ドメイン層 | `thiserror` の列挙体。変種ごとに code と分類を返す | 呼び出し側が `match` で網羅的に分岐でき、分類の書き忘れを型が落とす |
| アプリケーション境界 | 確定したかどうかと次の手を付けた失敗に写す。`anyhow` は使わない | 呼び出し側が文字列を読まずに次の手を選べる |
| 失敗の記録 | 結果を決める持ち主が1回だけ、型つきの event で出す | 同じ失敗を段ごとに重ねて出さない |

### 守ること

- ドメイン層で `anyhow::Error` を返さない。 畳んだ時点で網羅性が失われ、呼び出し側は文字列を見る以外の手段を持たなくなる。これが最大のアンチパターン。
- エラー列挙体に `#[non_exhaustive]` を付けない。 網羅性チェックを効かせるため。バリアントの追加は破壊的変更として扱う。
- 原因を捨てない。 下位のエラーは `#[source]` で繋ぐ。`.to_string()` して詰め直さない。
- `Display` は固定の文にする。 パス・音源名・歌詞・綴り・入力値を差し込まない。数や段のような安全な値は型つきの欄で持つ。
- code は型をまたいで一意にする。 別の型が同じ code を名乗らない。
- `unwrap()` は禁止（lint で deny）。 `expect()` はブートストラップ層でのみ使い、メッセージに「何を期待していたか」を書く。
- 失敗ではないものをエラーにしない。 「アライメントの確信度が低い」は人に確認させるための入力であって失敗ではないので、結果型で表してエディタへ回す。「部分音源で必要なサンプルが無い」も、選んだ方式のカバレッジでは通常起きうる状態なので、エラーにせずカバレッジ判定で事前に弾く。確定したあとの派生物の失敗（テイクは保存済みで解析だけ落ちた）も、確定した結果の一部として値で返す。エラー列挙体に入れてよいのは、呼び出し側が続行を諦める必要があるものだけ。

## トレース

### 守ること

- `#[tracing::instrument(err)]` を使わない。 `Display` を段ごとに重ねて記録し、表示文に入った値まで載る。失敗は結果を決める持ち主が、code・分類・確定の状態・段だけを型つきの event で1回出す（`DEC-PLT-038`）。
- `println!` / `eprintln!` / `dbg!` は禁止（lint で deny）。 出力は `tracing` に統一する。
- span は工程の単位で切る。 録音1テイク、アライメント1件、書き出し1回。利用者から見た「操作」と一致させる。
- `instrument` に載せるフィールドを選ぶ。 既定では引数が全部記録されるので、パスや歌詞を含む引数は `skip` する。

```rust
#[tracing::instrument(skip(pcm), fields(take_index, method = ?method))]
fn finalize_take(pcm: &[f32], take_index: usize, method: Method) -> Result<TakeId> { /* ... */ }
```

### 3つの出力段

1. `fmt` 層 — 開発時の人間向け。`RUST_LOG` で制御する。
2. ファイル層 — 利用者の端末にローカル保存する。障害報告に添付できる。
3. 送信層 — オプトインのときだけ有効。既定は無効。

### 送信層はホワイトリスト方式にする

ブラックリスト方式では必ず漏れる。 送ってよいフィールド名を列挙した定数を1箇所に置き、そこに無いものは通さない。

送ってはいけないもの: 音源名（＝キャラクター名。本人の創作物）、ファイルパス（利用者名を含む）、歌詞、プロジェクト名、波形。非公開のまま完成できることを担保した製品が、トレースで音源名を送っていたら意味がない。

エラーを送るときは code（`"recording.disk_full"` のような固定文字列）と分類だけを使い、エラー型にはそれを返すメソッドを持たせる。エラーの `Display` を送らない。固定の文にしてあっても、それは利用者に見せる文言で、送信の語彙ではない。

## lint

### 方針

`clippy::all` はリポジトリ全体で常に deny。 設定は `Cargo.toml` の `[workspace.lints]` に置き、各クレートは `[lints] workspace = true` で継承する。

各クレートの `lib.rs` に `#![deny(clippy::all)]` を書く方式は採らない。**クレートを追加したときに書き忘れると静かに無効化される**ためで、ワークスペース側に置けば継承が既定になる。CI の `cargo clippy -- -D warnings` は、設定が外れた場合の backstop。

`clippy::all` は `priority = -1` にしてあるので、個別の lint 指定が常に優先される。

新しいクレートを追加したら、必ず `[lints] workspace = true` を書く。

### 追加している lint

| lint | 水準 | 理由 |
|---|---|---|
| `clippy::unwrap_used` | deny | 回復可能な失敗を握り潰さない |
| `clippy::expect_used` | allow | ブートストラップ層で使うため |
| `clippy::print_stdout` / `print_stderr` / `dbg_macro` | deny | 出力は `tracing` に統一する |
| `clippy::todo` / `unimplemented` | warn | 実装中に手を止めない |
| `rust::unsafe_op_in_unsafe_fn` | deny | FFI（各 OS の音声 API、WORLD）で `unsafe` は避けられないので、範囲を明示させる |

`unsafe_code` そのものは禁止しない。各 OS の音声 API（windows-rs / coreaudio-rs / pipewire-rs）と WORLD への FFI が必須のため。代わりに `unsafe` ブロックには `// SAFETY:` コメントを必ず書く（`clippy::missing_safety_doc` が `clippy::all` に含まれる）。

### 例外の入れ方

行単位・ブロック単位の `#[allow(...)]` で入れる。理由をコメントで残す。

```rust
// FFI の戻り値は C 側で範囲が保証されている。
#[allow(clippy::cast_possible_truncation)]
let frames = raw_frames as usize;
```

クレート全体の `#![allow(...)]` は使わない。 範囲が広すぎて、後から入った本物の問題を隠す。

テストコードでの緩和は `clippy.toml` 側で設定済み（`allow-unwrap-in-tests`、`allow-expect-in-tests`、`allow-panic-in-tests`）。`dbg!` はテストでも禁止のままにしてある（消し忘れがそのまま入るため）。

## Tauri のコマンド

### 待つものは `#[tauri::command(async)]` にする

素の `#[tauri::command]` はメインスレッドで走る。 マクロが同期関数を
`kind.block(...)` で呼び出しスレッドのまま実行するので、そこで数秒かかると
WebView ごと止まる。

`async` を付けると `spawn_blocking` に載る。関数は同期のままでよい。

```rust
#[tauri::command(async)]
#[specta::specta]
pub fn finish_take(state: State<'_, AppState>) -> Result<TakeView> {
```

本体を書き換えない。 `async fn` にすると `std::sync::MutexGuard` が await を
跨げなくなり、`tokio::sync::Mutex` へ替える話になる。`command(async)` なら
await 点が無いので、その問題が起きない。

移すのは、録音・合成・ファイル・FFT・台帳（SQLite）に触るもの。
即返るものは移さない——スレッドプールへの往復が増えるだけで、短い間隔で
引くものはむしろ遅くなる。どれを残しているかは `commands.rs` の冒頭にある。

生成する TS は変わらない。 画面側から見た形は同じなので、
`bindings.gen.ts` に差分は出ない。

## コメント

言語を問わない規律は [writing-comments](../writing-comments/SKILL.md) が持つ。Rust 固有はここだけ。

### `# Errors` は条件を名指せるときだけ書く

書く価値があるのは「何が起きたら失敗するか」であって、「失敗すると `Err` が返る」ではない。呼び側が変種ごとに違う手を打てるなら書く。打てないなら書かない。

```rust
/// # Errors
///
/// `=` が無い、欄が足りない、数値として読めない。
```

`clippy::missing_errors_doc` は入れていない。`koeru-audio` の状態機械は16の遷移がすべて「手順の前提を満たしていない」で失敗し、`koeru-core` の多くは I/O をそのまま畳んで返す——lint を通すために16回同じ文を書くと、本当に条件を持つ `koeru-align` の記述まで読み飛ばされる。

現状は `koeru-align` が 28/28、境界の `koeru-app` が 6/67。この差は意図したもの。

## 依存を追加するとき

**`koeru-model` は依存を許可リストで持つ。** 並べていない crate を足すと `crates/koeru-model/tests/purity.rs` が落ちる。M6 では同じ規則を WASM から呼ぶ（`DEC-PLT-034`）ので、入出力・SQLite・外部形式の読み書き・時計は外側の crate が持つ。本体のコードで `std::fs` / `std::io` / `std::time` などに触っても落ちる（試験の module は時間を測ってよい）。

KOERU は AGPL-3.0-or-later。 許可リストは `deny.toml` にあり、`cargo deny check` が機械判定する。非商用限定（CC BY-NC 系）、再配布禁止、独自条項のものは通らない。

**学習済みモデルやデータセットはより慎重に見る。** モデル側の表示ライセンスが、学習に使われたコーパスの条件を上書きできるとは限らない。**「モデルに CC BY と書いてあるから大丈夫」は根拠にならない。** この判断で一度候補を落としている（MFA 日本語音響モデル）。

**ただし禁止ではない。** 重みが明示的に許諾的なライセンスで配られており、コーパスの条件が及ぶかが法的に定まっていない場合は、**判断で通してよい。通すなら、コーパスの状態と「判断で通した」ことを判断記録に残す**（例: `DEC-SYN-004`）。黙って通さない。

## 検証

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

CI で同じものを実行する。
