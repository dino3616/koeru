---
name: verify-koeru
description: KOERU の検証手順。手元で何をどの順に走らせるか、CI が何を見ているか、実音声で確かめるべきものは何かを定める。書いていない OS 向けの組み立て、WebView 側の検査、仕様側（fslc / xtask）の検査、Nix の検査、アプリの起動を含む。変更を検証するとき、CI が落ちた原因を切り分けるときに使う。環境の用意そのものは setup-koeru が持つ。
---

# KOERU — 検証

手元で通してから出す。 ここに挙げたものは CI（`.github/workflows/ci.yml`）が同じものを実行する。

**環境の用意は `setup-koeru` が持つ。** Nix の導入、direnv、git-lfs と submodule の
順序、Nix で覆えないものはそちら。ここは「揃っている前提で何を走らせるか」だけ。

## devShell の中で走らせる

ツールは `flake.nix` が供給する（`DEC-PLT-033`）。 direnv を入れていれば
ディレクトリに入った時点で揃っている。入れていないなら明示的に包む。

```bash
nix develop                                   # 対話で入る
nix develop --command cargo test --workspace  # 1つだけ走らせる
```

以下のコマンドは、すべてこのシェルの中で走らせる。

## 基本

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check
```

## 実行した件数を見る

`cargo test` の緑は「実行した」ではない。 CI が判定に使うのは受領証で、試験 binary を
1本ずつ走らせて件数を `meta/suites/` の登録と突き合わせる（`DEC-PLT-039`）。

```bash
cargo xtask test-receipt               # 組み立てて走らせ、登録と突き合わせる
cargo xtask test-receipt --runner bun  # UI の story 試験（ui の中の `bun run test`）
cargo xtask check-portfolio            # 登録の漏れだけを見る。組み立てない
```

落ちる形は4つ。 登録の無い試験 binary、`min_cases` を割った実行件数、失敗、
登録（`manual`）より多い無視。 受領証は `target/receipts/{実行器}-{OS}-{backend}.json`。

試験ファイルを足したら、`meta/suites/` にも1件足す。 `min_cases` は、その試験が
件数を求められるどの環境でも下回らない数にする（書いていない OS では cfg で外れる
ものがある）。

**手動のハーネスは `#[ignore]`。** マイク・出力・実音声が要るものは既定では走らない。
手元の実機で `-- --ignored` を付けて走らせ、前提が無ければ落ちる。

```bash
cargo test -p koeru-audio --test record_to_file -- --ignored --nocapture
cargo test -p koeru-app --test vertical_slice -- --ignored --nocapture
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

組んだ Kaldi が動くことは `kaldi_build` が見る。 モデルを開いて特徴の次数と
フレーム数を突き合わせるので、C++ の組み方が壊れれば落ちる。
**精度は見ていない。** ツールチェーンを変えたときに気づくためのもので、
境界が合っているかとは別（`DEC-PLT-033`）。

アライメントは実音声で見る。 合成音の試験は構造しか見ておらず、
**位置が全部ずれていても1つも落ちない**（CMVN の分散正規化で踏んだ。`EVID-ALN-001`）。
録音を1つ通して、パワーで見た発声区間と重なるかを確かめる。回帰テストではないので
`#[ignore]` にしてあり、環境変数を指して `--ignored` で走らせる。指していなければ落ちる。

```bash
KOERU_ALIGN_SAMPLE_WAV=/path/to/take.wav \
KOERU_ALIGN_SAMPLE_READING='ぎ ぎゃ ぎゅ ぎょ' \
  cargo test --package koeru-align --test alignment_on_real_audio -- --ignored --nocapture
```

**歌わせるところも実音声で見る。** 周波数表の当て方を間違えると、
単体試験は全部通ったままアプリだけが雑音を出す（`DEC-SYN-008`）。
指定した音高で鳴るかと、雑音になっていないかを見る。これも回帰テストではない。

```bash
KOERU_SYNTH_SAMPLE_WAV=/path/to/take.wav \
KOERU_SYNTH_SAMPLE_OFFSET_MS=1235 KOERU_SYNTH_SAMPLE_LENGTH_MS=550 \
  cargo test --package koeru-synth --test preview_on_real_audio -- --ignored --nocapture
```

## WebView 側

すべて `crates/koeru-app/ui` の中で完結する。 モノレポにしていないので、
ワークスペースを跨ぐ設定は無い。

```bash
cd crates/koeru-app/ui
bun install
bun run check           # 整形・lint・型（vp check --fix）
bun run check:ci        # 直さずに見る ＋ 試験（CI と同じ）
bun run build           # ビルド ＋ 型 ＋ npm のライセンス
```

試験は story だけ。 実ブラウザに描いて axe を当て、`play` で
axe に規則が無い性質を見る（`TR-PLT-25`、`DEC-PLT-022`）。
試験ファイルを別に置いていないので、`src/__tests__/` は無い。

`check:ipc` は `api` の呼び方を見る。 `api.…()` に `.then` / `.catch` /
`.finally` を繋いでいたら落ちる——その場で状態を持ち直している合図で、
押して走るものは `useMutation`、読みは `~/lib/queries` へ寄せる（移行中の形。`DEC-PLT-037`）。

検査範囲は story の範囲そのもの。 部品に story が無ければ一度も検査されない。
配色の段も `src/styles/palette.story.tsx` に並べたものだけが測られる。

**ブラウザは devShell が供給する。** `PLAYWRIGHT_BROWSERS_PATH` が
nixpkgs の `playwright-driver.browsers` を指しているので、
**`playwright install` を走らせない**——入れ直すと NixOS と NixOS-WSL で動かない。

`package.json` の版と nixpkgs の版が揃っていないと、入っていないブラウザを探して落ちる。
CI が突き合わせて落とす。揃うまで npm 側を上げない（`DEC-PLT-033`）。

```bash
echo "$KOERU_PLAYWRIGHT_FROM_PACKAGE_JSON / $KOERU_PLAYWRIGHT_FROM_NIXPKGS"
```

目で見るなら `bun run storybook`。

npm の依存ライセンスも見る。 Rust は `cargo deny check`、npm は
`check:licenses`。以前は npm 側に検査が無かった。

配色の検査を飛ばさない。 段を選び直したまま出すと、明暗どちらかで WCAG 2.2 AA を割る。
**網羅は機械で見ていない。** 測られるのは `src/styles/palette.story.tsx` に手で並べた
組み合わせだけで、`src/` で使いはじめた段をそこへ足し忘れても検査は通る。段を使いはじめたら足す。
（`PAIRS` という表で網羅を見ていると書いていたが、その表は無かった。）

`vp` の範囲を `ui/` の外へ広げない。 外すと `meta/` の TOML を畳み直して差分を濁らせる。**一度やった。**

生成物は整形と lint から外してある（`vite.config.ts` の `GENERATED`）。
`src/routeTree.gen.ts` は TanStack Router、`src/lib/bindings.gen.ts` は Rust から出る。

lint の規則は、`lint.plugins` に載っているプラグインのぶんだけ効く。
規則名だけ書いてもプラグインを挙げていなければ黙って無視される。 一度そうなった
（`typescript/no-explicit-any` を書いたのに `oxlint` を直接叩いたときだけ落ちた）。

### 画面へ渡す型（生成物）

移行中の生成物で、Rust のコマンド定義から作る（契約の正本は canonical SDL、`DEC-PLT-035`）。 `bindings.gen.ts` を手で直さない。

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
cargo xtask check-meta          # meta の参照先が実在するか
cargo xtask check-budgets       # 配分の合計が上限を超えていないか
cargo xtask check-coverage      # 全要件に技術が当たっているか
cargo xtask check-references          # 文書とコメントの ID 参照が実体に解決するか
cargo xtask index-decisions --check   # 判断記録の索引が古くないか
cargo xtask check-profile <ID>  # 未決の論点がリリースを塞いでいないか
```

## レビューに入る前に、触れた契約を出す

変更が引いている ID を、本文ごと出す。 検査ではなく、読む前の準備。

```bash
cargo xtask touched            # main との差分
cargo xtask touched origin/dev # base は変えられる
```

出るのは、要件なら条文と確信度、判断なら選んだ案と**覆る条件**。
レビューで見るのは「この変更が引き金を引いていないか」なので、条件を並べないと誰も見ない。

**拾うのは差分の hunk の中だけ。** 変更されたファイルの ID を全部拾うと、
`AGENTS.md` を2行直しただけでそこに並ぶ数十件が出る。文脈の幅はコードで 25 行、
文書で 3 行——コードの引用は doc コメントにあって離れており、文書の引用は行そのものにある。

**出るのはコメントが引いているものだけ。** 引用が無い変更ファイルは別に並ぶ。
そこは機械では対応が出ないので、読んで決める。末尾の数が、変更の何割を語れているかを示す。

人が読む前の下ごしらえにも、レビューする側へ渡す材料にも使う。

```bash
cargo xtask touched > /tmp/brief.md
```

記録を足すときは番号を `next-id` で取る。 検査ではないが、ここに置いておく。

```bash
cargo xtask next-id DEC-PLT     # その接頭辞で、まだ使われていない番号
```

ディレクトリを見て決めない。 既にある ID へ上書きすると、そこにあった判断が
消え、それを引用していた記録だけが別のことを指す。形も参照先も壊れないので、
上の検査は全部通る。**踏んだ**（`meta/README.md`）。

`check-profile` は通常の CI では走らせない。 未決が残っているのは開発中は正常で、
その状態でリリースするのが異常だという線引きにしている。
実行は `.github/workflows/release-gate.yml`。

仕様を書き換えたら、変異検査で空洞になっていないかを見ること。
生き残りは失敗ではなく、レビュー待ちの列として扱う。

```bash
fslc mutate specs/requirements/project-lifecycle.fsl --depth 8
```

`fslc` はバージョンと SHA-256 で固定している（`flake.nix`）。
**hash を手で書き換えない。** 版を上げたら次を走らせる。

```bash
bash .github/scripts/update-hashes.sh   # 上流の公開 digest から 6 個すべてを作り直す
```

CI では `nix-hash-fix` が PR の中で同じものを当てる（`DEC-PLT-033`）。
FSL 内部の crate を直接 import せず、CLI の JSON 出力だけに依存する。

## Nix 側

`flake.nix` も整形と lint の対象。 整形の契約を持たないファイルを作らない。

```bash
nix fmt                                              # 整形する
git ls-files -z '*.nix' | xargs -0 nixfmt --check    # 整形されているか
git ls-files -z '*.nix' | xargs -0 -n1 statix check -o errfmt
git ls-files -z '*.nix' | xargs -0 deadnix --fail
nix flake check                                      # flake が評価できるか
```

`statix` は `-o errfmt` で出す。 既定の span 描画は日本語コメントの桁を
バイト数で数えるので、指している場所がずれて読めない。

**`systems` を足したら、`bunAssets` と `fslcAssets` にも足す。** 忘れると
評価が落ちる。退避経路は置いていない——黙って fslc の無い devShell を配るより、
組み立たないほうが良い。
