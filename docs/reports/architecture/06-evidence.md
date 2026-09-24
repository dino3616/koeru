# KOERU 調査 Evidence Register

## 調査点と確信度

調査日: 2026-09-23 (Asia/Tokyo)。main は `fb331897f41166a3ec1a2a5c440ff38f773d00b6`。Open PR は #16、head は `af9a808bff296f35b38bb4049c2ffc43788bfe33`。同 PR の GitHub 合成 merge は `983bc9b0a3b33bae1d6dfb9fffc728afcef81091`。main と PR head を区別して読み、変更後を設計入力にした。将来 merge される最終 SHA がこれらと一致することまでは仮定しない。

GitHub 接続経由で仕様、登録簿、コード、PR metadata・review、CI 定義と最新 run/job/log を調査した。ローカル clone は環境の通信制約で失敗した。今回、KOERU の build/test、実機、実音声、聴感評価、migration を自分の環境では実行していない。静的コード照合、公開 CI 実行証跡、リポジトリに記録された実測を区別する。全行・全レビューを網羅した完全監査ではない。特に Windows/Linux の native 実装と人手品質基準は未検証である。

以下の `Rxx` はこの調査資料内だけの参照番号であり、repository の `DEC-*` / `EVID-*` と競合する新しい canonical ID ではない。コードの permalink は、特記なき場合 PR head に固定する。

## Repository primary sources

基底 URL: `https://github.com/dino3616/koeru/`

| ID | 読んだ場所 / 固定点 | 設計に使う事実・留保 |
|---|---|---|
| R01 | `AGENTS.md`、`meta/README.md`、`specs/README.md`、README、product vision | FSL は形式契約、meta は技術要件・決定・質問・evidence・budget・profile、Markdown は意図説明。生成文書を手で変更しない。Skill は既に5つ、`.agents/skills` が実体。 |
| R02 | PR #16 metadata、changed filenames、review threads、head `af9a808…` | 方式・多音階・曲取り込み・境界保存を同時に拡張。36 commits / 171 files。作者のチェックボックスは実行証明と同一視しない。 |
| R03 | `Cargo.toml`、各 `crates/koeru-{core,audio,align,synth,package,app}/Cargo.toml` | 6 production crates と xtask。core に Diesel/SQLite・ZIP・YAML、package に audio/align 依存。crate 名は純粋性を保証しない。 |
| R04 | `crates/koeru-app/src/commands.rs` 1–115、`studio.rs` の構成・open/finish | `AppState` は `Mutex<Studio>`。envelope/pending は長い確定操作を避けるため別 handle。command の async 指定だけでは業務操作の並列性は得られない。 |
| R05 | `crates/koeru-core/src/db.rs` 230–412、PR16 comment `4069534096` | 既存全行から tone 別 alias の claimed set を作り、新しい repacked row で重複 alias を保存しない。候補寄与と出力一意性を同じ所有権に押し込めている。コードとレビューを照合した静的 finding。 |
| R06 | `crates/koeru-app/src/studio.rs` 1980–2445、特に2090–2180 | WAV の後に take を DB commit し、その後 F0/analysis/frq/alignment/oto が続く。後半失敗を「録音全体失敗」と返すと durable outcome が曖昧になる。 |
| R07 | `crates/koeru-app/src/workers.rs` 1–220 | unbounded BinaryHeap、単一実行 thread、Box<FnOnce>、global cancel flag、Drop join。優先順位は明示されているが running native work の強制中断ではない。 |
| R08 | `crates/koeru-app/ui/src/lib/queries.ts` 1–250、`ipc.ts`、`use-recorder.ts`、voice screen | project ID 付き cache key と、project 引数なしの backend API が共存。`openProjectQuery` が open/session mutation を query として実行。gcTime=0 は causality の保証ではない。 |
| R09 | `crates/koeru-core/src/project.rs` 430–560、`db.rs` WAL open | snapshot と derive は DB 本体を `fs::copy`。derive の音声 copy は直下の file のみ。WAL snapshot の一貫性、nested tone assets、途中生成物の扱いを再設計する必要。全 snapshot が実際に壊れたという実測ではない。 |
| R10 | `crates/koeru-audio/src/wav.rs`、`specs/design/project-storage.fsl` | WAV は part→sync→rename→DB。ディレクトリ同期等を実際の durable contract に含める必要。FSL は orphan を本人が採用/破棄するまで保存し、自動修復しない。 |
| R11 | `crates/koeru-audio/src/ring.rs` 1–225 | SPSC preallocation/atomic/drop counters は意図的。Producer/Consumer が自動 Sync、push/pop が &self であり、safe caller が複数 producer/consumer を作れて unsafe proof の前提を破れる。API soundness の静的 finding、race は今回未実行。 |
| R12 | `crates/koeru-app/src/pump.rs` 1–260、`audio/backend/macos/capture.rs` | native rate を44100へ一度変換、preroll/tail を sample clock で管理。envelope・clip count は非RT側で集計。capture callback は preallocated scratch と atomics。 |
| R13 | `audio/backend/macos/playback.rs` 1–205、320–end | writer が RwLock<Vec> を拡張し callback が try_read。待機はしないが、競合時に無音を出し starved を増やす。既再生 sample も Vec に残る。 |
| R14 | `crates/koeru-app/src/error.rs` | AppError は kind/message。送信可能 kind と同端末表示 message は既に区別されている。一方 source chain と caller action は折り畳まれる。 |
| R15 | `meta/requirements/editor.toml`、`meta/profiles/PROFILE-M6.toml` | absolute sample model、per-boundary provenance/confirmation、reversible commands、invalid numeric/import drafts、manual protection、non-destructive import、外部変更監視、bounded charts/undo。一括編集は12種に加えて13番目の条件選択＋制限されたフィールド式が記載されている。任意コード scripting とは別。 |
| R16 | `meta/profiles/PROFILE-M7.toml`、platform requirements | 3 OS 配布・署名・自動更新・accessibility・performance・利用計測。profile が planned であることと実装済みを区別。 |
| R17 | `meta/decisions/DEC-TEL-001.toml`、`meta/questions/Q-TEL-001.toml`、`specs/requirements/telemetry-consent.fsl` | accepted は既定オフ、SaaS、self-hosted server なし。local logs は consent と独立。telemetry/crash report は別同意、application scope、最初の録音後に訊く。SaaS 未選定は M7 blocker。 |
| R18 | `meta/budgets/BUDGET-MEMORY-001.toml`、`BUDGET-LATENCY-001.toml`、`SCALE-REF-001.toml` | peak1500MB、editor chart416MB/undo200MB 等は allocation で未実測。30s 初回試唱枠。latency に旧 miniaudio と古い align 見積もりが残る。数値の存在≠実証。 |
| R19 | PR #3 description / review context | 0 tests green、CSS 未適用 axe、空 parent tsc、await 漏れ、trace scanner 漏れ、DEC ID 上書き、Suspense waterfall が記録される。修正済みの教訓を現在のバグ件数に加算しない。 |
| R20 | PR #7 reviews: `4039646788`、`4039646791`、`4039646797`、retake flow 指摘 | aggregate score を偽の成分に復元、confirmed pins の read omission、historical take 表示に対して adopted take を mutation。レビュー時点の問題。全項目の現 head 再実行はしていない。 |
| R21 | `meta/evidence/EVID-ALN-001.toml`、`align/tests/alignment_on_real_audio.rs` | CMVN variance normalization は synthetic structural tests 全通過のまま実音声を壊した記録。real-audio harness は macOS cfg、env/model 未設定で return。IoU sanity は境界精度ラベル評価ではない。 |
| R22 | `.github/workflows/ci.yml`、`release-gate.yml`、UI package/config、binding/offline tests | FSL/meta/generation/license/unsupported backend/Storybook guards。UI step 名 vp check だが実体は `bun run check:ci`。名前だけから story 未実行と判定しない。 |
| R23 | CI run `35699288861`、Ubuntu job `106653212184` log | Sep22 の最新調査 run は failure。Ubuntu で offline trace whitelist test が packaging の `findings` field に反応。safe 件数であり内容漏洩そのものではない。cargo は途中で止まるため後続 crate 合格を推測しない。 |
| R24 | CI phonemizer-parity job、PR16 旧 parity 指摘 | stable/alpha の固定 release/hash、方式ごとの3 voicebanks を使う。以前の「方式別でない」指摘は現コードでは修正された教訓として扱う。 |
| R25 | PR16 custom presamp discussion `4066610267` / author `4066793493` | 外部 rules 変更時の detect/confirm/restrict/reindex の挙動は product decision を要する。architecture が黙って auto-reindex を選ばない。 |
| R26 | `specs/requirements/editor-constraints.fsl` | 11制約をviolation booleanへ抽象化し、3msはモデル対象外、historyはfinite abstraction。modeが許可operationへ影響するためnormal/advancedは純React view stateではない。 |

固定 permalink の書式: `https://github.com/dino3616/koeru/blob/af9a808bff296f35b38bb4049c2ffc43788bfe33/<path>`。main 固有の履歴は `fb331897f41166a3ec1a2a5c440ff38f773d00b6` を使う。

レビューの入口: `https://github.com/dino3616/koeru/pull/16#discussion_r4069534096`、`https://github.com/dino3616/koeru/pull/7#discussion_r4039646797`、`https://github.com/dino3616/koeru/pull/3`。
CI: `https://github.com/dino3616/koeru/actions/runs/35699288861`。

## 外部 primary sources と使い方

| ID | 出典 | 使う範囲 / 採用しない飛躍 |
|---|---|---|
| X01 | PortAudio, Writing a Callback: `https://www.portaudio.com/docs/v19-doxydocs/writing_a_callback.html` | callback に allocation/I/O/mutex を置かない制約の確認。PortAudio を KOERU に採用する根拠ではない。 |
| X02 | rust-analyzer Architecture: `https://rust-analyzer.github.io/book/contributing/architecture.html` | input/derived separation、semantic API と transport、immutable Analysis snapshot。Salsa、panic cancellation、全 crate 構成を移植しない。 |
| X03 | SQLite WAL / Online Backup: `https://www.sqlite.org/wal.html`、`https://www.sqlite.org/backup.html` | WAL は DB の一部。live copy を backup API に替える根拠。audio file まで SQLite が atomic にするわけではない。 |
| X04 | Tokio spawn_blocking: `https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html` | started blocking task は abort できず shutdown timeout でも停止しない。native cancellation は別契約。 |
| X05 | TanStack Query cancellation: `https://tanstack.com/query/latest/docs/framework/react/guides/query-cancellation` | AbortSignal は backend durable side effect の rollback ではない。Suspense hooks の cancellation 制限を考慮。 |
| X06 | Tauri WebDriver: `https://v2.tauri.app/develop/tests/webdriver/` | platform 別 native testing path。現英語 docs の macOS WDIO 経路と、direct tauri-driver の対応範囲を区別。test server/plugin を production に入れない。 |
| X07 | Audacity project repair tools README: `https://github.com/audacity/audacity-project-tools/blob/main/README.md` | AUP3/SQLite 世代の実運用復旧例。SQLite 導入だけで破損や欠落が消えるという主張の反証。Audacity 全版の現在構造を一般化しない。 |
| X08 | Zimmermann et al., ICST 2011, An Empirical Study of the Factors Relating Field Failures and Dependencies; Microsoft Research 公開 abstract | Vista と Eclipse で依存複雑性との関係が異なる。依存数を減らすと必ず defect が減るという普遍則は採らない。本文 PDF 未読、因果推論の証拠として使わない。 |
| X09 | GraphQL Specification September 2025: `https://spec.graphql.org/September2025/` | Query/Mutation/Subscription root、subscription operationが1 root fieldであること、execution/error/null/deprecationの規範。GraphQLをHTTP transportと同一視しない。 |
| X10 | `async-graphql` Schema docs: `https://docs.rs/async-graphql/latest/async_graphql/struct.Schema.html` | in-process `execute` / `execute_stream` と runtime SDL exportが可能。HTTP/WebSocket serverは必須ではない。Rust static schema ergonomicsはRust type/macro中心であることを実装コストとして扱う。 |
| X11 | GraphQL Code Generator client preset: `https://the-guild.dev/graphql/codegen/plugins/presets/preset-client` | typed Query/Mutation/Subscription documents、Fragment Masking、component近傍のfragment colocationを支える。Apollo normalized cache採用の根拠ではない。 |
| X12 | GraphQL Inspector validate docs: `https://the-guild.dev/graphql/inspector/docs/commands/validate` | executable documentsをschema againstで検査しdeprecated usageも検出できる。特定CI SaaS採用の根拠ではなく、必要なfitness functionの実現可能性。 |
| X13 | Apollo Compiler executable validation: `https://docs.rs/apollo-compiler/latest/apollo_compiler/executable/index.html` | Rust側でSDL/executable documentを独立parse/validateできる選択肢。async-graphql runtime declarationをcanonical sourceにしないparity checkに利用可能。 |
| X14 | Tauri IPC Channel docs: `https://docs.rs/tauri/latest/tauri/ipc/struct.Channel.html` | `Channel::send`をgeneric GraphQL execution stream transportとして使える。application-specific Channel contractを残す根拠ではない。 |

外部資料から採ったのは制約・反例・検査方法である。GraphQL採用判断はX09–X14だけに依存せず、R04/R08のcurrent contract ownership、M6のcross-feature projection圧力、future consumer horizonと比較して行った。`async-graphql`、Codegen、Inspector/Apollo Compiler等の具体toolは交換可能で、canonical SDL/Query-Mutation-Subscription/fragment ownershipというarchitecture contractをtool名と同一視しない。

## Finding の分類

| 分類 | 例 | 扱い |
|---|---|---|
| 現 main に静的に存在 | playback try_read、SPSC safe API、snapshot copy | 実装 defect / durability gap として最初の検証 task へ。再現済みとは記さない。 |
| Open PR 後に存在 | alias claimed ownership による repacked row 欠落 | code+review が一致。candidate/selection 境界の再設計理由。 |
| 歴史的に修正された教訓 | PR3 false greens、非2冪 ring、CMVN、方式別 parity | regression suite の感度条件。現在の未修正バグに数えない。 |
| 未決定 product | custom rules の扱い、外部 WAV 変換、zero-cross 詳細、SaaS | Q/DEC と人間へ返し、default を architecture 都合で作らない。 |
| 未確認 / false positive 候補 | review bot の他の未照合警告、履歴/DCO 指摘 | evidence が無ければ確定 finding にしない。修正不要とも断定しない。 |

## 検証の次の担当者が必ず再確認すること

実 merge SHA と差分、全 required CI jobs、canonical SDL/runtime SDL parity、GraphQL operations/fragments/manifestの実収集件数、Subscription actual event receipts、coverage の実行件数、real fixtures のライセンスと hash、native OS の availability、DSP/聴感、power-loss を含む supported filesystem durability、旧 project の migration corpus。これらは本 report の設計推論を production evidence に変えるための work であり、今回実行済みとするものではない。
