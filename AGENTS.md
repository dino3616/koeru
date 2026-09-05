# AGENTS.md

このリポジトリでエージェントが作業するときの前提。人間向けの入口は [README.md](README.md)、貢献の手順は [CONTRIBUTING.md](CONTRIBUTING.md)。

## このリポジトリは何か

KOERU — UTAU 向けの歌声ライブラリ制作スタジオ。録音から配布パッケージ生成までを一つのプロジェクトで扱い、録音の途中でも自分の声で歌を聴けることを中核に置く。

M2 を実装中。 録音 → テイク確定 → 試唱までが動く。実装より先に、触る領域の文書を読むこと。 要件と FSL が先にあり、コードはその写しになる。

正本は3面に分かれている。どれが何を持つかを取り違えないこと。

| 面 | 何の正本か | 場所 |
|---|---|---|
| FSL | 形式的な契約。状態・遷移・不変条件・受入・禁止 | `specs/` |
| Meta | 決定・未決の論点・調査 Evidence・実測予算・リリース対象 | `meta/` |
| Markdown | 背景・物語・意図・調査の説明 | `docs/` |

`docs/` の各文書は [README.md](README.md) が一覧している。`docs/product-vision.md` は確定している方針で、ここに反することはしない。 `docs/generated/` は FSL からの生成物で、手で編集しない。

技術的なことは `docs/` には無い。 ID の住所は次のとおり。

- `TR-*` → [meta/requirements/](meta/requirements/) ／ `DEC-*` → [meta/decisions/](meta/decisions/) ／ `Q-*` → [meta/questions/](meta/questions/)
- `PROFILE-M1`〜`M7` → [meta/profiles/](meta/profiles/)（要件はちょうど1つに属する） ／ `BUDGET-*` → [meta/budgets/](meta/budgets/) ／ `EVID-*` → [meta/evidence/](meta/evidence/)

読み方と規律は [meta/README.md](meta/README.md) と [specs/README.md](specs/README.md)。

**meta にファイルを足すときは、先頭で `schema` を宣言する。** 名乗らないファイルは検査で落ちる。ディレクトリの中身から形を推測すると、打ち間違えたファイルが黙って0件を貢献する。

## Skills

正本は `.agents/skills/`。`.claude/skills/` は symlink。 追加・編集は必ず `.agents/skills/` 側で行い、`.claude/skills/` には symlink を張るだけにする。実体を両方に置かない。

このリポジトリが持っているのは4つ。

| Skill | いつ使うか |
|---|---|
| `writing-comments` | コメントや説明文を書く・直すとき。言語を問わない。何を書き何を書かないか、要件の引用の形 |
| `rust-conventions` | Rust のコードを書く・直す・レビューするとき。 エラー型、tracing、clippy、依存追加の方針 |
| `react-conventions` | 画面を書く・直すとき。tailwind-variants、`className` を受けないこと、部品の粒度、Rust との境界 |
| `verify-koeru` | 変更を検証するとき。何をどの順に走らせるか、CI が何を見ているか |

次の skill はリポジトリの外にある（保守者の環境やプラグイン由来）。clone しただけでは付いてこない。
無い環境では、`specs/README.md` と `fslc --help` を読んでから書くこと。

| Skill | いつ使うか |
|---|---|
| `fsl` | `specs/` の FSL を書く・直すとき。言語仕様、検証器、反例からの修復手順 |
| `fsl-requirements` / `fsl-design` | 要求層 / 設計層を自然言語から起こすとき |

FSL を書く前に、形式化メモをチャットに出して確認を取ること。 出典に無い要件を推測で埋めない。`fslc` が保証するのは「書かれたモデルの内部整合性」であって、モデルが KOERU の意図を正しく表しているかは人が確かめる。

## 破ってはいけないもの

1. ドメイン層で `anyhow::Error` を返さない。 `thiserror` の列挙体を返す。畳むのはアプリケーション境界だけ。詳細は `rust-conventions` skill。

2. `println!` / `eprintln!` / `dbg!` を使わない。 出力は `tracing` に統一する。lint で deny されている。例外は実機ハーネス（`tests/guide_leak.rs` など）だけで、そこはファイル先頭の `#![allow(clippy::print_stdout)]` に理由を添える。`dbg!` はテストでも禁止のまま。

3. トレースに音源名・ファイルパス・歌詞・プロジェクト名を載せない。 送信フィールドはホワイトリスト方式にする。送ってよいフィールド名を列挙した定数を1箇所に置き、そこに無いものは通さない。KOERU は「非公開のまま完成できる」ことを担保している製品なので、これが漏れると前提が崩れる。

**4. 依存を追加するときはライセンスを確認する。** AGPL-3.0-or-later に取り込めるものだけ。許可リストは `deny.toml`、判定は `cargo deny check`。**学習済みモデルは、モデル側の表示ライセンスが学習コーパスの条件を上書きできるとは限らない。** これは注意であって禁止ではない。**通すなら、コーパスの状態と「判断で通した」ことを判断記録に残す**（例: `DEC-SYN-004`）。黙って通さない。

5. コミットには `Signed-off-by` を付ける。 `git commit -s`。DCO を採用している。CI が全コミットを検査する。

6. FSL と meta が所有する命題を、手書き文書で再定義しない。 同じ規則を2箇所に書くと、片方だけが変わる。手書き文書には ID で参照させる。

> 悪い例: `F0 推定は SwiftF0 を採用する。`
> 良い例: `F0 推定の方針は DEC-SYN-001 を参照。`

**7. 生成文書を手で直さない。** `docs/generated/` は FSL から決定論的に生成している。**Markdown から FSL への逆同期はしない。** 直すのは常に FSL 側で、文書は再生成する。CI が drift を検出して落とす。`background` スロットの中だけは自由に書ける。

8. `docs/` に日付を書かない。 これらは継続的に参照するリファレンスで、決定ログではない。「2026年時点」「現在」のような時点表現も避け、常に現在を語る文書として書く。方針が変わったら該当行を書き換える。履歴は残さない。

## 検証

手順は `verify-koeru` skill が持つ。 ここには最短の入口だけ置く。

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

先に `git-lfs` を入れてから submodule を取る。 取らずに clone すると途中で死ぬ。

書いていない OS 向けの組み立ても手元で通す。
音声のバックエンドは macOS しか無く、他 OS では `backend/unsupported.rs` が選ばれる。
見ないと、アプリが組み立たないことに CI で初めて気づく。**一度やった。**

```bash
F='--cfg koeru_force_unsupported_backend'
RUSTFLAGS="$F" cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTFLAGS="$F" RUSTDOCFLAGS="$F" cargo test --workspace --all-features
```

`RUSTDOCFLAGS` を忘れない。 `RUSTFLAGS` は rustdoc に届かないので、
doctest だけが違う設定でコンパイルされ、存在しないはずの差分で落ちる。

アライメントと合成は実音声で見る。 合成音の試験は構造しか見ておらず、
位置が全部ずれていても1つも落ちない（`EVID-ALN-001`）。手順は `verify-koeru`。

WebView 側、アプリの起動、仕様側（`fslc` / `cargo xtask`）も `verify-koeru` にある。

## 実装で押さえておくこと

- 実装は Rust + Tauri。 単一のネイティブアプリ、PC 前提。処理はローカル完結で、声をサーバへ送らない
- 音声 I/O は各 OS の API を直接叩く（`DEC-REC-001`）。必要なのは OS 側の音声加工を無効化する経路（排他モード、または共有モード＋ RAW ストリーム要求）へ到達できることで、`cpal` はそのどちらにも降りられない。抽象レイヤも採らない。 TR-REC-08〜12 が要求する制御をどの抽象も出さず、省けるのはデバイス列挙とコールバックの配管だけだった。束ねるのは windows-rs / coreaudio-rs / pipewire-rs
- アライナは MFA の日本語音響モデルを Kaldi 経由で叩く（`DEC-ALN-008`）。Python は要らない。 OpenFst も引かない——`OPENFST_VER=10800` を定義し、`hmm/transition-model.h` が引く `fst/fst-decl.h` に空スタブを置けば、推論に要る8モジュールは通る（`EVID-ALN-001`）
- **MFA のモデルは 16kHz 前提。** KOERU のマスターは 44100 Hz なので、アライメントの入口でダウンサンプルが要る。**特徴は LDA+MLLT の 40 次で、`meta.json` の `uses_splices` / `uses_deltas` は当てにならない**（バイナリを読んで確かめた）
- MFCC の dither は 0 にしてある。 `meta.json` は 1 だが、乱数を足すので `TR-ALN-29` の「ビット単位で同一」と両立しない。戻さないこと
- MFA の topology は音素の飛び越しを許す。 `k` も `a` も3状態だが `HmmTopology::MinLength` は 1。「状態数＝最短の継続長」ではない（`EVID-ALN-001`）
- **1パス目は `final.alimdl`（話者非依存）。** `final.mdl` は SAT で fMLLR 済みの特徴を前提にしており、素の特徴に当てると尤度が歪む。**踏んだ**
- `ComputeFmllrDiagGmm` は使えない。 ヘッダに宣言があるだけで Kaldi に定義が無く、リンクで初めて分かる。実際の口は `FmllrDiagGmmAccs::Update`
- **`koeru-align` にも「書いていない OS」の席がある**（`src/mfa/unsupported.rs`）。**trait を足したら両方に実装すること。踏んだ**
- 合成は WORLD ベース。 ニューラルボコーダへの置き換えは採らない（「あなたの声そのもの」が「生成された声」に変わるため）
- フロントは shadcn に依存しない。 レジストリからコードを写すだけで、実体は自前実装になる（`DEC-PLT-015`）
- 部品は `className` を props で受けない。tailwind-merge も使わない（`~/lib/tv` は `twMerge: false`）。受けないなら衝突が起きず、畳む処理は空回りする。詳細は `react-conventions` skill
- 画面へ渡す型と呼び出し口は Rust から生成する（`DEC-PLT-019`）。`ui/src/lib/bindings.gen.ts` は手で直さない
- 画面へ流し続けるものは Tauri の Channel で送る。`invoke` で引きに行かせない（`DEC-PLT-017`）。`invoke` は応答の順序を保証しないので、引きに行くと波形が巻き戻る。流し続けるものはアプリの状態ロックの外から読む（テイク確定中は数秒握られる）
- 収録画面の状態機械は `ui/src/lib/use-recorder.ts` が持つ。画面は組み立てだけ。二重確定を避ける札（`TR-REC-42`）と連続収録のループは描画と別の寿命で回るので、`useRef` で持つ——state にすると押すたびに描き直す
- `Card` の見出しの段は入れ子の深さが決める。段を props で渡さない。渡すと、部品を移したときに数え直しを忘れて `h2` の中に `h2` が入る
- 配色は Radix Colors の段の意味を守る。 1=地、2=面、…11=低コントラストの字、12=高コントラストの字。塗りは段 9 ではなく段 11（段 9 は明暗で同じ値になる色があり、字を載せると 4.5:1 に届かない）。検査は `crates/koeru-app/ui/scripts/check-contrast.ts`
- リングの位置は総数で持つ。剰余で持たない（`DEC-REC-007`）。剰余で持つと容量が2の冪のときにしか合わず、環をまたぐたびに古い音を読み直す。リングの試験を2の冪の容量だけで書かない
- **マスターは常に 44100 Hz。** キャプチャはネイティブレートで受け、**pump が1回だけ変換する**（`TR-REC-02`、`DEC-REC-006`）。**そこから下流へレートを持ち回さない。**`write_distribution` はヘッダに 44100 と書くだけでサンプルを変換しないので、**マスターが 44100 でないまま配ると、44100 と名乗る別のレートの音になる**（踏んだ）
- **周波数表は素材ファイル全体を `.frq` の格子（hop=256）で渡す。切り出さない**（`DEC-SYN-008`）。oto での切り出しと 5ms 格子への載せ替えは合成器がする。**切り出し済みのつもりで渡すと、offset 手前の無声フレームが発声の先頭に当たり、声が雑音になって音高も乗らない**（踏んだ）
- **`koeru-synth` の `RenderRequest.tone` は「鳴らしたい音高」。収録音高ではない。** ここに収録音高を渡すと、どの音高を選んでも同じ高さで鳴る（**踏んだ**）
- FSL の `const` を写す前に、契約か仮定かを見る。 `ASSUME-` で始まる前置きが「検証用に有限へ閉じる」と言っているものは製品の規則ではない（`MAX_TAKES` を写して、3テイクで収録が止まった。`DEC-REC-005`）
- 未決の論点は `meta/questions/` が正本。 メモリ予算に触るものは実装着手前に数値を積み直す。どれがリリースを塞いでいるかは `cargo xtask check-profile <ID>` が出す
- FSL 化してあるのは縦切り1本だけ（録音 → テイク確定 → 完成 → 非公開のまま終了 → ZIP 書き出し）。技術要件を一度に FSL へ移さないこと。形式化できない文章まで入れると、FSL が新しい巨大文書になる
- `fslc` はバージョンと SHA-256 で固定している（CI 参照）。更新は Renovate 任せにせず、semantic diff を確認してから上げる。FSL 内部の crate を直接 import せず、CLI の JSON 出力だけに依存する

## 注意

`.claude/skills/` の symlink は、Windows で `core.symlinks` が無効だとパス文字列のプレーンファイルとして展開される。その場合は `.agents/skills/` 側を直接参照すること。
