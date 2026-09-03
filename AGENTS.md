# AGENTS.md

このリポジトリでエージェントが作業するときの前提。**人間向けの入口は [README.md](README.md)、貢献の手順は [CONTRIBUTING.md](CONTRIBUTING.md)。**

## このリポジトリは何か

**KOERU** — UTAU 向けの歌声ライブラリ制作スタジオ。録音から配布パッケージ生成までを一つのプロジェクトで扱い、**録音の途中でも自分の声で歌を聴けること**を中核に置く。

**M2 の実装に入った。** 音声層（`crates/koeru-audio`）から書き始めている。**実装より先に、触る領域の文書を読むこと。** 要件と FSL が先にあり、コードはその写しになる。

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
| [meta/requirements/](meta/requirements/) | 満たさなければ成立しない条件。`TR-*` |
| [meta/decisions/](meta/decisions/) | 何を選び、なぜ選び、何が起きたら覆すか。`DEC-*` |
| [meta/questions/](meta/questions/) | まだ決まっていないこと。`Q-*` |
| [meta/profiles/](meta/profiles/) | マイルストーン。**要件はちょうど1つに属する**。`PROFILE-M1`〜`M7` |
| [meta/budgets/](meta/budgets/) | 上限と配分（`BUDGET-*`）、領域ごとの性能目標（`{領域}.toml`） |
| [meta/evidence/](meta/evidence/) | 判断の根拠。調査資料、実測、部品台帳（`components.toml`） |
| [specs/](specs/) | 状態・遷移・不変条件・受入・禁止。反例探索にかけている |

読み方と規律は [meta/README.md](meta/README.md) と [specs/README.md](specs/README.md)。

**meta にファイルを足すときは、先頭で `schema` を宣言する。** 名乗らないファイルは検査で落ちる。ディレクトリの中身から形を推測すると、打ち間違えたファイルが黙って0件を貢献する。

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

**4. 依存を追加するときはライセンスを確認する。** AGPL-3.0-or-later に取り込めるものだけ。許可リストは `deny.toml`、判定は `cargo deny check`。**学習済みモデルは、モデル側の表示ライセンスが学習コーパスの条件を上書きできるとは限らない。** これは注意であって禁止ではない。**通すなら、コーパスの状態と「判断で通した」ことを判断記録に残す**（例: `DEC-SYN-004`）。黙って通さない。

**5. コミットには `Signed-off-by` を付ける。** `git commit -s`。DCO を採用している。CI が全コミットを検査する。

**6. FSL と meta が所有する命題を、手書き文書で再定義しない。** 同じ規則を2箇所に書くと、片方だけが変わる。手書き文書には ID で参照させる。

> 悪い例: `F0 推定は SwiftF0 を採用する。`
> 良い例: `F0 推定の方針は DEC-SYN-001 を参照。`

**7. 生成文書を手で直さない。** `docs/generated/` は FSL から決定論的に生成している。**Markdown から FSL への逆同期はしない。** 直すのは常に FSL 側で、文書は再生成する。CI が drift を検出して落とす。`background` スロットの中だけは自由に書ける。

**8. `docs/` に日付を書かない。** これらは継続的に参照するリファレンスで、決定ログではない。「2026年時点」「現在」のような時点表現も避け、常に現在を語る文書として書く。**方針が変わったら該当行を書き換える。履歴は残さない。**

## 検証

**最初に git-lfs を入れてから submodule を取る。** WORLD と Kaldi は submodule で調達しており
（`DEC-PLT-016`）、**MFA の音響モデルは HuggingFace のリポジトリを submodule にしている**
（`DEC-ALN-012`）。モデルの実体は LFS なので、`git-lfs` が無いと
`git-lfs filter-process: command not found` で clone が途中で死ぬ。**一度やった。**

```bash
brew install git-lfs && git lfs install
git submodule update --init --recursive
```

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
cargo deny check   # 要 cargo install cargo-deny --locked

```

**書いていない OS 向けの組み立ても手元で通す。** 音声のバックエンドは macOS しか無く、
他 OS では `backend/unsupported.rs` が選ばれる。これを見ないと、アプリが組み立たないことに
CI で初めて気づく。**一度やった。**

```bash
F='--cfg koeru_force_unsupported_backend'
RUSTFLAGS="$F" cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTFLAGS="$F" RUSTDOCFLAGS="$F" cargo test --workspace --all-features
```

**`RUSTDOCFLAGS` を忘れない。** `RUSTFLAGS` は rustdoc に届かないので、
doctest だけが本物と違う設定でコンパイルされ、**存在しないはずの差分で落ちる。**
（クロスコンパイルには C のツールチェーンが要り、手元では通せない。これが代わり。）

**アライメントは実音声で見る。** 合成音の試験は構造しか見ておらず、
**位置が全部ずれていても1つも落ちない**（CMVN の分散正規化で踏んだ。`EVID-ALN-001`）。
録音を1つ通して、パワーで見た発声区間と重なるかを確かめる。**回帰テストではない**ので、
環境変数が無ければ静かに戻る。

```bash
KOERU_ALIGN_SAMPLE_WAV=/path/to/take.wav \
KOERU_ALIGN_SAMPLE_READING='ぎ ぎゃ ぎゅ ぎょ' \
  cargo test --package koeru-align --test alignment_on_real_audio -- --nocapture
```

**歌わせるところも実音声で見る。** 周波数表の当て方を間違えると、
**単体試験は全部通ったままアプリだけが雑音を出す**（`DEC-SYN-008`）。
録音を1つ通して、指定した音高で鳴るかと、雑音になっていないかを見る。
これも**回帰テストではない**。

```bash
KOERU_SYNTH_SAMPLE_WAV=/path/to/take.wav \
KOERU_SYNTH_SAMPLE_OFFSET_MS=1235 KOERU_SYNTH_SAMPLE_LENGTH_MS=550 \
  cargo test --package koeru-synth --test preview_on_real_audio -- --nocapture
```

WebView 側は別に検証する。**すべて `crates/koeru-app/ui` の中で完結する。**
モノレポにしていないので、ワークスペースを跨ぐ設定は無い。

```bash
cd crates/koeru-app/ui
bun install
bun run check           # 整形・lint・型（vp check --fix）
bun run build           # ビルド ＋ 型 ＋ **配色の検査**
```

**配色の検査を飛ばさない。** 段を選び直したまま出すと、明暗どちらかで WCAG 2.2 AA を割る。

**`vp` の範囲を `ui/` の外へ広げない。** 外すと `docs/generated/` を整形して
FSL の drift 検出を落とし、`meta/` の TOML を畳み直して差分を濁らせる。**一度やった。**
`src/routeTree.gen.ts` は TanStack Router の生成物なので、整形と lint から外してある。

アプリを動かす。

```bash
cd crates/koeru-app/ui && bun run build   # 先にフロントを作る
cargo run --package koeru-app
# または HMR 込みで
cd crates/koeru-app/ui && bun run tauri dev
```

**`vite.config.ts` の `environments.ssr` を消さない。** TanStack Start の dev サーバは
`ssr` 環境の中でサーバ入口を実行して HTML を返す。vite-plus の既定の `ssr` 環境は
それができない形なので、**Start は黙って middleware を入れず、画面が「Cannot GET /」だけになる。**
**一度やった。** `createRunnableDevEnvironment` は **`vite` から取る**——
`vite-plus` が再輸出するものは別のクラスを作り、Start からは走らせられない環境に見える。

仕様側は別に検証する。

```bash
fslc lint specs/ --project specs/fsl-project.toml   # ID 規約
fslc chain specs/fsl-project.toml                   # 各層の検証と、層の継ぎ目の refine
fslc document check specs/requirements/project-lifecycle.fsl docs/generated/project-lifecycle.md
cargo xtask check-meta                                 # meta の参照先が実在するか
cargo xtask check-budgets                              # 配分の合計が上限を超えていないか
cargo xtask check-coverage                             # 全要件に技術が当たっているか
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
crates/koeru-core/  ドメイン層。GUI と OS に依存しない。台帳・録音リスト・プロジェクト・書式
crates/koeru-audio/ 音声 I/O。各 OS の API を直接叩く。状態機械は recording-input.fsl の写し
crates/koeru-synth/ 合成。submodule の WORLD への FFI、resampler
crates/koeru-align/ 自動原音設定。MFA のモデルを Kaldi 経由で叩き、5値を導く
crates/koeru-align/vendor/kaldi/  submodule。**推論に要る8モジュールだけ cc で組む**
crates/koeru-align/models/japanese_mfa/  submodule（HuggingFace、LFS）。MFA の音響モデルと辞書
crates/koeru-app/   アプリケーション層。**Rust も WebView 側もここ1つに入っている**
crates/koeru-app/src/   Tauri のコマンドと、縦切りの組み立て
crates/koeru-app/ui/    WebView 側。React + TanStack Start（SPA）+ Tailwind + Radix
                        **bun のパッケージはここで完結する。** package.json も bun.lock もこの中
xtask/              仕様ゲート。meta と FSL / 技術要件の ID を突き合わせる
specs/              FSL の正本。requirements / design / refinement
meta/               要件・判断・未決の論点・Evidence・予算・リリースプロファイル
docs/               設計文書
docs/generated/     FSL から生成した文書。手で編集しない
Cargo.toml          [workspace.lints] で clippy::all を deny
deny.toml           AGPL 互換ライセンスの許可リスト
```

## 実装で押さえておくこと

- **実装は Rust + Tauri。** 単一のネイティブアプリ、PC 前提。処理はローカル完結で、声をサーバへ送らない
- **音声 I/O は各 OS の API を直接叩く**（`DEC-REC-001`）。必要なのは OS 側の音声加工を無効化する経路（排他モード、または共有モード＋ RAW ストリーム要求）へ到達できることで、`cpal` はそのどちらにも降りられない。**抽象レイヤも採らない。** TR-REC-08〜12 が要求する制御をどの抽象も出さず、省けるのはデバイス列挙とコールバックの配管だけだった。束ねるのは windows-rs / coreaudio-rs / pipewire-rs
- **アライナは MFA の日本語音響モデルを Kaldi 経由で叩く**（`DEC-ALN-008`）。**Python は要らない。** OpenFst も引かない——`OPENFST_VER=10800` を定義し、`hmm/transition-model.h` が引く `fst/fst-decl.h` に空スタブを置けば、推論に要る8モジュールは通る（`EVID-ALN-001`）
- **MFA のモデルは 16kHz 前提。** KOERU のマスターは 44100 Hz なので、アライメントの入口でダウンサンプルが要る。**特徴は LDA+MLLT の 40 次で、`meta.json` の `uses_splices` / `uses_deltas` は当てにならない**（バイナリを読んで確かめた）
- **MFCC の dither は 0 にしてある。** `meta.json` は 1 だが、乱数を足すので `TR-ALN-29` の「ビット単位で同一」と両立しない。**戻さないこと**
- **MFA の topology は音素の飛び越しを許す。** `k` も `a` も3状態だが `HmmTopology::MinLength` は 1。**「状態数＝最短の継続長」ではない**（`EVID-ALN-001`）
- **1パス目は `final.alimdl`（話者非依存）。** `final.mdl` は SAT で fMLLR 済みの特徴を前提にしており、素の特徴に当てると尤度が歪む。**踏んだ**
- **`ComputeFmllrDiagGmm` は使えない。** ヘッダに宣言があるだけで Kaldi に定義が無く、リンクで初めて分かる。実際の口は `FmllrDiagGmmAccs::Update`
- **`koeru-align` にも「書いていない OS」の席がある**（`src/mfa/unsupported.rs`）。**trait を足したら両方に実装すること。踏んだ**
- **合成は WORLD ベース。** ニューラルボコーダへの置き換えは採らない（「あなたの声そのもの」が「生成された声」に変わるため）
- **フロントは shadcn に依存しない。** レジストリからコードを写すだけで、実体は自前実装になる（`DEC-PLT-015`）
- **配色は Radix Colors の段の意味を守る。** 1=地、2=面、…11=低コントラストの字、12=高コントラストの字。**塗りは段 9 ではなく段 11**（段 9 は明暗で同じ値になる色があり、字を載せると 4.5:1 に届かない）。検査は `crates/koeru-app/ui/scripts/check-contrast.ts`
- **マスターは常に 44100 Hz。** キャプチャはネイティブレートで受け、**pump が1回だけ変換する**（`TR-REC-02`、`DEC-REC-006`）。**そこから下流へレートを持ち回さない。**`write_distribution` はヘッダに 44100 と書くだけでサンプルを変換しないので、**マスターが 44100 でないまま配ると、44100 と名乗る別のレートの音になる**（踏んだ）
- **周波数表は素材ファイル全体を `.frq` の格子（hop=256）で渡す。切り出さない**（`DEC-SYN-008`）。oto での切り出しと 5ms 格子への載せ替えは合成器がする。**切り出し済みのつもりで渡すと、offset 手前の無声フレームが発声の先頭に当たり、声が雑音になって音高も乗らない**（踏んだ）
- **`koeru-synth` の `RenderRequest.tone` は「鳴らしたい音高」。収録音高ではない。** ここに収録音高を渡すと、どの音高を選んでも同じ高さで鳴る（**踏んだ**）
- **FSL の `const` を写す前に、契約か仮定かを見る。** `ASSUME-` で始まる前置きが「検証用に有限へ閉じる」と言っているものは製品の規則ではない（`MAX_TAKES` を写して、3テイクで収録が止まった。`DEC-REC-005`）
- **未解決の設計課題が2件ある**（`meta/questions/Q-PLT-001` と `Q-EDT-001`）。いずれもメモリ予算の破綻で、実装着手前に数値を積み直す必要がある
- **FSL 化してあるのは縦切り1本だけ**（録音 → テイク確定 → 完成 → 非公開のまま終了 → ZIP 書き出し）。技術要件を一度に FSL へ移さないこと。形式化できない文章まで入れると、FSL が新しい巨大文書になる
- **`fslc` はバージョンと SHA-256 で固定している**（CI 参照）。更新は Renovate 任せにせず、semantic diff を確認してから上げる。FSL 内部の crate を直接 import せず、CLI の JSON 出力だけに依存する

## 注意

`.claude/skills/` の symlink は、Windows で `core.symlinks` が無効だとパス文字列のプレーンファイルとして展開される。その場合は `.agents/skills/` 側を直接参照すること。
