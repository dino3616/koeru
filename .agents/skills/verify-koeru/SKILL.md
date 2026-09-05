---
name: verify-koeru
description: KOERU の検証手順。手元で何をどの順に走らせるか、CI が何を見ているか、実音声で確かめるべきものは何かを定める。git-lfs と submodule の用意、書いていない OS 向けの組み立て、WebView 側の検査、仕様側（fslc / xtask）の検査、アプリの起動を含む。変更を検証するとき、CI が落ちた原因を切り分けるとき、環境を用意するときに使う。
---

# KOERU — 検証

手元で通してから出す。 ここに挙げたものは CI（`.github/workflows/ci.yml`）が同じものを実行する。

## 最初に git-lfs を入れてから submodule を取る

WORLD と Kaldi は submodule で調達しており（`DEC-PLT-016`）、**MFA の音響モデルは
HuggingFace のリポジトリを submodule にしている**（`DEC-ALN-012`）。
モデルの実体は LFS なので、`git-lfs` が無いと
`git-lfs filter-process: command not found` で clone が途中で死ぬ。**一度やった。**

```bash
brew install git-lfs && git lfs install
git submodule update --init --recursive
```

モデルが無くてもアプリは動く。 自動原音設定が音響モデルを使わない退避経路に落ちるだけ。

## 基本

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check   # 要 cargo install cargo-deny --locked
```

## 書いていない OS 向けの組み立ても手元で通す

音声のバックエンドは macOS しか無く、他 OS では `backend/unsupported.rs` が選ばれる。
これを見ないと、アプリが組み立たないことに CI で初めて気づく。**一度やった。**

```bash
F='--cfg koeru_force_unsupported_backend'
RUSTFLAGS="$F" cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTFLAGS="$F" RUSTDOCFLAGS="$F" cargo test --workspace --all-features
```

`RUSTDOCFLAGS` を忘れない。 `RUSTFLAGS` は rustdoc に届かないので、
doctest だけが本物と違う設定でコンパイルされ、存在しないはずの差分で落ちる。
（クロスコンパイルには C のツールチェーンが要り、手元では通せない。これが代わり。）

## 実音声で見るもの

アライメントは実音声で見る。 合成音の試験は構造しか見ておらず、
**位置が全部ずれていても1つも落ちない**（CMVN の分散正規化で踏んだ。`EVID-ALN-001`）。
録音を1つ通して、パワーで見た発声区間と重なるかを確かめる。回帰テストではないので、
環境変数が無ければ静かに戻る。

```bash
KOERU_ALIGN_SAMPLE_WAV=/path/to/take.wav \
KOERU_ALIGN_SAMPLE_READING='ぎ ぎゃ ぎゅ ぎょ' \
  cargo test --package koeru-align --test alignment_on_real_audio -- --nocapture
```

**歌わせるところも実音声で見る。** 周波数表の当て方を間違えると、
単体試験は全部通ったままアプリだけが雑音を出す（`DEC-SYN-008`）。
指定した音高で鳴るかと、雑音になっていないかを見る。これも回帰テストではない。

```bash
KOERU_SYNTH_SAMPLE_WAV=/path/to/take.wav \
KOERU_SYNTH_SAMPLE_OFFSET_MS=1235 KOERU_SYNTH_SAMPLE_LENGTH_MS=550 \
  cargo test --package koeru-synth --test preview_on_real_audio -- --nocapture
```

## WebView 側

すべて `crates/koeru-app/ui` の中で完結する。 モノレポにしていないので、
ワークスペースを跨ぐ設定は無い。

```bash
cd crates/koeru-app/ui
bun install
bun run check           # 整形・lint・型（vp check --fix）
bun run check:ci        # 直さずに見る ＋ 試験（CI と同じ）
bun run build           # ビルド ＋ 型 ＋ 配色 ＋ npm のライセンス
```

試験は story だけ。 実ブラウザに描いて axe を当て、`play` で
axe に規則が無い性質を見る（`TR-PLT-25`、`DEC-PLT-022`）。
試験ファイルを別に置いていないので、`src/__tests__/` は無い。

検査範囲は story の範囲そのもの。 部品に story が無ければ一度も検査されない。
配色の段も `palette.stories.tsx` に並べたものだけが測られる。

実ブラウザなので Playwright が要る。 CI では `~/.cache/ms-playwright` を
lockfile のハッシュでキャッシュしている。手元では `bunx playwright install chromium`。

目で見るなら `bun run storybook`。

npm の依存ライセンスも見る。 Rust は `cargo deny check`、npm は
`check:licenses`。以前は npm 側に検査が無かった。

配色の検査を飛ばさない。 段を選び直したまま出すと、明暗どちらかで WCAG 2.2 AA を割る。
検査は比だけでなく網羅も見る——`src/` で使っている段が `PAIRS` に無ければ落ちる。

`vp` の範囲を `ui/` の外へ広げない。 外すと `docs/generated/` を整形して
FSL の drift 検出を落とし、`meta/` の TOML を畳み直して差分を濁らせる。**一度やった。**

生成物は整形と lint から外してある（`vite.config.ts` の `GENERATED`）。
`src/routeTree.gen.ts` は TanStack Router、`src/lib/bindings.gen.ts` は Rust から出る。

lint の規則は、`lint.plugins` に載っているプラグインのぶんだけ効く。
規則名だけ書いてもプラグインを挙げていなければ黙って無視される。 一度そうなった
（`typescript/no-explicit-any` を書いたのに `oxlint` を直接叩いたときだけ落ちた）。

### 画面へ渡す型（生成物）

正本は Rust のコマンド定義（`DEC-PLT-019`）。 `bindings.gen.ts` を手で直さない。

```bash
# コマンドや境界の型を足したら作り直す
KOERU_WRITE_BINDINGS=1 cargo test -p koeru-app --test bindings

# 古くなっていないか見るだけ（CI はこちら）
cargo test -p koeru-app --test bindings
```

作り直したら、書いていない OS 向けでも同じものが出ることを確かめる。
出るものが変わるなら、バックエンド固有の型が境界へ漏れている。

```bash
RUSTFLAGS="--cfg koeru_force_unsupported_backend" cargo test -p koeru-app --test bindings
```

## アプリを動かす

```bash
cd crates/koeru-app/ui && bun run build   # 先にフロントを作る
cargo run --package koeru-app
# または HMR 込みで
cd crates/koeru-app/ui && bun run tauri dev
```

`vite.config.ts` の `environments.ssr` を消さない。 TanStack Start の dev サーバは
`ssr` 環境の中でサーバ入口を実行して HTML を返す。vite-plus の既定の `ssr` 環境は
それができない形なので、**Start は黙って middleware を入れず、画面が「Cannot GET /」だけになる。**
**一度やった。** `createRunnableDevEnvironment` は **`vite` から取る**——
`vite-plus` が再輸出するものは別のクラスを作り、Start からは走らせられない環境に見える。

## 仕様側

```bash
fslc lint specs/ --project specs/fsl-project.toml   # ID 規約
fslc chain specs/fsl-project.toml                   # 各層の検証と、層の継ぎ目の refine
fslc document check specs/requirements/project-lifecycle.fsl docs/generated/project-lifecycle.md
cargo xtask check-meta          # meta の参照先が実在するか
cargo xtask check-budgets       # 配分の合計が上限を超えていないか
cargo xtask check-coverage      # 全要件に技術が当たっているか
cargo xtask check-references          # 文書とコメントの ID 参照が実体に解決するか
cargo xtask index-decisions --check   # 判断記録の索引が古くないか
cargo xtask check-profile <ID>  # 未決の論点がリリースを塞いでいないか
```

`check-profile` は通常の CI では走らせない。 未決が残っているのは開発中は正常で、
その状態でリリースするのが異常だという線引きにしている。
実行は `.github/workflows/release-gate.yml`。

仕様を書き換えたら、変異検査で空洞になっていないかを見ること。
生き残りは失敗ではなく、レビュー待ちの列として扱う。

```bash
fslc mutate specs/requirements/project-lifecycle.fsl --depth 8
```

`fslc` はバージョンと SHA-256 で固定している（CI 参照）。
更新は Renovate 任せにせず、semantic diff を確認してから上げる。
FSL 内部の crate を直接 import せず、CLI の JSON 出力だけに依存する。
