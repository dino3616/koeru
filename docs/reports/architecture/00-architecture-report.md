# KOERU: 長期境界を導く architecture redesign

**状態: 調査に基づく設計提案。repository の accepted decision でも、実装・性能検証済みという報告でもない。**

本書は調査→仮説→反証→比較→結論の順に記述する。固定 SHA、証拠の分類、リンクは `06-evidence.md`、日常の判断は `01-contributor-guide.md`、検証と観測は `02-tests-observability.md`、移行は `03-task-dag.md`、具体例は `07-worked-examples.md`、canonical GraphQL contract の詳細は `08-graphql-application-contract.md` を参照。

## 1. 何を調査し、何を確認できたか

main `fb331897…` と唯一の Open PR #16 head `af9a808…` を区別して、FSL/meta、crate dependencies、Rust/React、IPC、録音・解析・合成・配布・保存、workers、test/CI、レビュー履歴と実測記録を読んだ。最新 CI の Ubuntu log も確認した。ローカル実行・実機・聴感・migration 実行はしていない。以下で「存在」は静的確認、「記録」は repository にある記述、「提案」は本設計を意味する。[R01–R25]

### 1.1 Documentation は三面の正本

FSL は状態・遷移・不変条件の正本、meta は TR/DEC/Q/EVID/budget/profile の正本、Markdown は背景・意図の説明である。product vision は確定方針である。FSL が証明するのはモデルであり、SQLite、OS の flush、callback、React の実行がそのモデルを実現していることではない。[R01]

今回の architecture を採用するときは、新しい decision を `cargo xtask next-id` で確保して meta に記録し、必要な FSL refinement を human review する。handbook が FSL や meta の命題を新たに二重定義してはいけない。さらに canonical `schema.graphql` を consumer-facing application contract の正本として置く。ただし SDL は FSL/meta の上位ではない。FSL/meta が product semantics、SDL が consumer に約束する vocabulary/capability/projection を所有する。本調査文書は履歴を含む外部 report であり、そのまま evergreen docs 全体へコピーすることを推奨しない。

### 1.2 実際の architecture

```text
React routes/screens/components
  -> global lib/queries + ipc facade
  -> generated Tauri command bindings
  -> commands.rs: conversions + AppState Mutex<Studio>
  -> Studio: library/project/recording/review/analysis/preview/export orchestration
       -> Ledger (Diesel/SQLite) + ProjectDir/files
       -> native capture + Pump + session
       -> alignment / synthesis + Workers + caches
       -> packaging

callback -> preallocated capture ring -> Pump -> WAV
                            (AppState mutex は callback に入らない)
```

6 production crates があるが、core は pure domain に限らず SQL・archive・song parsing を含む。package は audio/align に依存する。application と desktop adapter が同じ crate にあり、画面への DTO と内部 command の形が近い。Studio の集中は単なる長いファイルではなく、異なる lifetime と成功境界を一つの mutable object に集めていることが問題である。[R03–R08]

意図された良い境界もある。callback と disk pump の分離、master clock への一度の変換、型生成、tracing の privacy allowlist、view と domain の用語差、native API への到達、FSL/meta の機械検査である。これらを名前変更のために壊さない。[R01,R10–R14,R19,R22]

### 1.3 偶然の境界を示す反例

| 観測 | 何が混ざっているか | 判断 |
|---|---|---|
| tone 内の alias を最初の row が claim | 録音素材の候補寄与と export の一意性 | PR16 の短い曲別 row に alias が付かない。candidate と selection を分ける。[R05] |
| project ID は query key にあるが backend request にない | cache identity と mutation/read target | gcTime=0 や mount 順序では A→B→A の遅延実行を防げない。[R08] |
| take commit 後も finish が解析全体を返す | durable capture と derived readiness | 保存済みなのに失敗と見え、retry の意味が曖昧。[R06] |
| editor/review の鍵が alias と採用 take に偏る | 表示したもの・編集したもの・採用したもの | 過去レビューの historical take 誤更新と pins 欠落が説明できる。[R20] |
| WAV sync/rename と SQLite を一つの成功に見る | filesystem と DB の耐障害性 | snapshot の live DB copy は WAL を含む snapshot ではない。[R09,R10] |
| callback は no-lock と説明しながら output は try_read | 待たないことと timing isolation | 競合時の無音・Vec 成長を止める別設計が必要。[R13] |
| test binary が存在、CI が green | discovery、backend、fixture、assertion execution | real-audio の early return、歴史的0件 green は別の保証が必要。[R19,R21] |

SPSC の safe API が複数 producer/consumer を許す点は直接の soundness defect 候補である。native resource の ownership を型で表す必要性を示すが、全 domain primitive を newtype にする根拠ではない。[R11]

### 1.4 Review をどう扱ったか

PR16 の alias claim は head の実コードと一致する。方式別 parity の過去指摘は現 CI が3方式・2channelを分離しているため修正済みの教訓とする。PR3 の false greens は今日すべて残っているとは言わない。PR7 の score 復元・historical take 問題は regression scenario として採るが、全経路を本調査で再実行したとは言わない。custom presamp の意味変更は product 未決定として残す。未照合 bot comment は確定 defect に数えない。[R05,R19,R20,R24,R25]

最新の調査 CI run は赤で、Ubuntu では `findings` 件数の tracing field が許可リスト外で落ちた。これは内容漏洩の証拠ではなく、実際に guard が働いた証拠である。逆に native test binary が0件の Linux 実行を、実機録音成功と扱わない。[R23]

## 2. 仮説: 最も強い変更圧力は何か

処理の順番ではなく、次の決定が一緒に変わる。

1. **素材の意味と解決。** 行の組み替え、音高、alias 表現、phonemizer、選択候補、preview/export coverage は「どの素材がどの要求を満たすか」で結び付く。表示文字列では分離できない。
2. **人が決めた値の保護。** M6 のドラッグ、数値入力、確認、bulk、external import、undo、再推定は同じ編集規則と provenance を必要とする。React と Rust に別の business rule を置くと同時変更が常態化する。[R15]
3. **不可逆な成功の境界。** 録音確定、release 出力、migration、snapshot は filesystem/DB の部分成功を共通して説明する必要がある。ただし共通 generic transaction framework は不要で、各 operation の protocol を明示する。
4. **時間・資源・受理。** native device、callback、project writer、CPU job、view は別の寿命と失敗特性を持つ。cancel は停止要求であり、commit の取消でも、FFI の強制停止でもない。[X01,X04]
5. **consumer-independent な意図。** 今はGUIが一つでも、UI の mount を所有権の根拠にすると追加 consumer が project を取り違える。必要なのは server 化ではなく明示された application context。
6. **検証の実行証跡。** M7 の3OS対応、M6のquality、privacyはtest名やcountsではなく、backend/fixture/commit/観測値が揃った証跡を必要とする。[R16,R17,R21–R23]

外部例の使い方にも限界を置く。rust-analyzer の input/derived と snapshot API は2・5の説明に有用だが、音声を全量 memory に載せたり Salsa の panic cancellation を callback に移したりしない。Audacity の AUP3 repair は DB を使っても recovery が必要な反例。依存複雑性と障害の関係は研究でも project ごとに異なり、crate 数や依存数の最小化を目的関数にしない。[X02,X07,X08]

## 3. 有力案の敵対的比較

| 観点 | A: 現状を保ち局所修正 | B: invariant owner + 明示 runtime | C: 全域 ports/actors/event architecture | D: daemon + universal API + event sourcing |
|---|---|---|---|---|
| cohesion | 今日の実装には高い、M6編集規則が漏れやすい | 意味・編集・確定・資源で高い | 層/メッセージ数が business cohesion を薄め得る | consumer はまとまるが音声資源は別世界 |
| coupling | Studio・暗黙 current project に残る | transport と engine を限定境界へ | interface/event order と DI 設定へ移る | protocol/version/event evolution へ移る |
| invariant owner | 現状は多点、追加disciplineが必要 | model が判断、project が受理・確定 | actor横断の保証が難しい | replayable domain と files の非atomic性が残る |
| runtime | lock 分割だけでは head-of-line blocking | RT/control/write/compute を分離 | queue増加、ordering/backpressureの総量増 | process isolation は良いが IPC/bulk data 費用 |
| test/diagnosis | 短期安価、時間競合を見落としやすい | replayable commandsとreceiptが安い seam | mock契約が増え、integrationが不可避 | replayは強いが端末/ファイル故障を代替しない |
| contributor負荷 | 最小の導入負荷、将来全体理解が必要 | owner/admission/rules を学ぶ固定費 | 小変更にtrait/message/configが増える | プロトコルと運用知識が大きい |
| OSS maintainability | 当面良い、専門領域の混線が継続 | 少数の実境界・小さな既存xtaskで強制 | contributor少数では抽象維持が重い | server/plugin公開の契約維持が重い |
| API evolution | GUI構造に引きずられる | capability DTOのみ互換境界 | interfaceの過剰public化に注意 | 多consumerには強いが今は未確定 |
| migration cost | 最小 | identity/persistence/APIの一度の大変更 | 大 | 最大、保存モデルの再定義が必要 |
| M6/M7 | パッチで達成可だが編集/競合が増える | 直接載る | 可能だが過剰 | 可能だが要件外の仕事が大きい |
| future consumer | current project と Tauri 型が障害 | canonical GraphQL schema/executorへadapterを足せる | adapter追加容易 | 最も容易だが先払い過多 |
| 危険な仮定 | 良い規約だけでglobal stateを制御できる | owner/admission設計を小さく維持できる | メッセージ化すれば低結合になる | 将来remote/pluginが中核要件になる |

**A が正しい可能性:** repository が小規模で M6 が限定的なら、移行費を払うより contracts を Studio 内に導入する方が安い。ただし semantic identity、result admission、editor kernel、commit receipt を導入すると実質 B に近付く。crate 名を維持した A でもこの境界があればよい。

**B が間違っている可能性:** ProjectRuntime が第二の巨大 Studio になる、command の粒度が細かすぎる、WASM adapter の運用費が便益を超える。このため writer は DB/受理だけ、能力別 module が決定と orchestration を持ち、RT/CPU work を移さない。WASM は5値編集だけ、初期spikeを採否gateにする。

**C が正しい可能性:** 複数の独立 backend、独立チーム、動的差し替えが実際に増えれば ports の価値は増す。しかし pure function や一つのSQLite実装まで trait 化する必要はない。現在はメッセージと owner を必要箇所だけ採用する。

**D が正しい可能性:** remote/multi-user/plugin isolation が正式要件になれば process boundary が必要になる。しかし process boundary/event sourcing と GraphQL application contract は別判断である。GraphQL は in-process execution と generic Tauri transportで採用し、daemon/event sourcing は採らない。自由な field selection 自体を目的にせず、Rust/Reactから独立した application vocabulary、feature-local projection、Query/Mutation/Subscription の分類を得る。

## 4. 推奨: 意味の owner と確定の ownerを分け、consumer contractをcanonical GraphQLへ分離する

**B を推奨する。** Clean/Hexagonal/CQRS の名称で規則を決めず、下記 ownership を契約にする。read projection と mutation intention は分けるが、別DB・event sourcing・CQRS framework は導入しない。consumer boundary は schema-first GraphQL とし、Tauri/Specta の Rust command surface を application contract の正本にしない。

```text
                     specs/ + meta/
                 product semantics/invariants
                           |
                           v
                 canonical schema.graphql
             Query / Mutation / Subscription
                     /             \
          React/Tauri              future MCP/automation/extension
                     \             /
                       koeru-graphql
                            |
                       Application
                            |
                       koeru-runtime
          project writer + result admission + durable protocols
           /         /         |          \          \
        model     formats    AudioHost    align       synth
      pure rules   codecs    control/RT  computation computation
          ^
  editor-wasm (M6 gesture preview only) <- React editor
```

GraphQL は domain/runtime の中心ではない。consumer に見える application language の owner である。identity、durability、RT safety、result admission は GraphQL より下位の invariant として残す。詳細は `08-graphql-application-contract.md`。

### 4.1 最終 crate と module

| crate | 責任 | 入れてはいけないもの / crate にする理由 |
|---|---|---|
| `koeru-model` | material/target resolution、editing/review decisions、capture domain transitions、export eligibility/planning | IO/SQLite/Tauri/GraphQL/React/native handles。native と editor WASM の両 target で同じ規則を使うため独立。 |
| `koeru-formats` | lossless OTO/UST/USTX 等の構文表現、encoding、WAV/FRQ、archive tree/codec | take adoption、project DB、GraphQL、native SDK、confidence policy。 |
| `koeru-runtime` | Application、library、project writer、coherent read session、persistence/recovery、recording/editing/material services、jobs、import/export、consent | Tauri/GraphQL/React、callback処理。consumer-independent implementation boundary。 |
| `koeru-graphql` | canonical SDL の Rust implementation、Query/Mutation/Subscription roots、GraphQL-specific DTO/scalar、schema parity、operation execution context | SQLite/Diesel/native audio/filesystem protocols/business rules。resolverはApplicationだけを呼ぶ。 |
| `koeru-audio` | device/control thread、capture/playback resource、RT buffers、capture clock/resampling/pump | project adoption、alias、GraphQL、review。OS SDK/unsafe/RT制約を隔離。 |
| `koeru-align` | MFA/Kaldi adapter、入力から観測境界・観測品質を算出 | manual pin、candidate selection、GraphQL resolver、UI queue。 |
| `koeru-synth` | WORLD等の分析・rendering adapter、bounded phrase computation | alias候補選択、user transpose判断、GraphQL resolver、export可否。 |
| `koeru-desktop` | composition root、generic GraphQL-over-Tauri transport、OS shell integration、React host/packaging | business invariant、DB schema、application-specific Tauri commands/channels。 |
| `koeru-editor-wasm` | M6の低遅延gesture preview用の薄いbinding | store/network/GraphQL/native audio/general plugin runtime。model::editingを別言語に再実装しない。 |

crate 数は目標ではない。GraphQL adapter は runtime と desktop の間に実際の dependency/consumer boundary が成立するため独立候補が強い。M6前に editor-wasm を空箱で作らない。旧 `core`/`package`/`app` と application-specific Specta/Tauri surface は段階移行後に除去し、恒久的な二重 contract を残さない。

runtime の module は例えば `application`, `project`, `read`, `storage`, `capture`, `editing`, `materials`, `jobs`, `importing`, `exporting`, `diagnostics`, `consent`。`koeru-graphql` は `query`, `mutation`, `subscription`, `types`, `scalars`, `schema` 程度に留め、第二の Studio を作らない。

### 4.2 不変条件の owner

| 不変条件 | 判断 owner | 確定 owner |
|---|---|---|
| material が要求phonetics/tone/methodを満たす | model::materials | project の selected candidate relation |
| 手編集・確認済み境界の保護、undo粒度 | model::editing | runtime::editing + project writer |
| waveform境界制約と操作別許容 | model::editing | native apply-edit。WASMは同じ判断のpreviewのみ |
| drop/xrun のある take の扱い | model::capture が要件に沿って決定 | capture finish receipt + project commit |
| DB take は確定済み asset を参照 | storage protocol | project writer、sealed asset token は外から構築不可 |
| export の一意性・全条件 | model の resolver/export plan | immutable export receipt |
| consentが有効な種別だけ送信 | application consent state machine | telemetry adapterの送信直前 gate |

「同じ規則を呼ぶ」と「同じ巨大 read model を全員に渡す」は違う。preview と export は共通 resolver を使い、要求する capability/用途条件は明示的に別にする。合成しない ending を export に含めるような既存仕様を単一booleanで潰さない。

## 5. Identity と durable truth

### 5.1 Identity を分ける

| 概念 | identity / 表現 / 所有関係 |
|---|---|
| Project | persisted ProjectId。開き直しの ProjectEpoch/lease は別値。A→B→Aでもepoch再利用しない。 |
| Recording target | phonetic sequence/context・方式上のrole・toneとの対応を保持。IDはrename/alias変更で変わらない。semantic equivalence判定は明示関数＋rules snapshot。 |
| Row | 録るための grouping/order/prompt。1 rowから複数material、同じtargetに複数rowが寄与できる。display textやrow ordinalをsemantic identityにしない。 |
| Capture | 録音開始ごとの CaptureId。stop/retry/rename を通して追跡。capture未確定でも存在できるが take row とは別。 |
| Take | 確定したimmutable masterと収録条件・妥当性。CaptureIdとの関係を保持。同一audio hashでも別録音は別take。 |
| Material candidate | TakeId＋source span＋target＋derivation revision を持つ CandidateId。候補の存在と採用は別。 |
| Setting entry | 編集対象の EntryId。candidate参照、5境界、provenance、confirmation、alias表現を持つ。履歴参照と採用参照を混ぜない。 |
| Selection | Target/用途/tonal context から CandidateId/EntryId への明示 relation。変更にはexpected selection revision。 |
| Alias | 可変の外部表現。空白・prefix・tone名を結合した文字列を主鍵にしない。phonetic reading は別の構造化データ。 |
| Song / Note | SongId と NoteId。歌詞・title・配列indexは主鍵ではない。複数モーラと note timing の対応を明示して取り込む。 |
| Export representation | export profile/method/encoding と exact source revisions から作る immutable plan。project設定をexportのために書き換えない。 |

単純な Hz/dB/count 全部をentity化しない。ID取り違え・timebase取り違え・寿命取り違えが本当に危険な型に絞る。sample span は対象 AssetId と rate/timebase を伴う。負 overlapやinvalid numeric draft を unsigned型で潰さない。

手編集を retake に引き継ぐ既存ルールは保持する。ただし古い WAV の絶対位置を新しい WAV に無条件copyしてはいけない。Entry の人の意図、candidate固有のanchor、引継ぎの根拠を分け、採用済み仕様に沿う明示 transform を使う。仕様が不足する部分は migration task で human decision として止める。

### 5.2 何が真実か

**durable:** immutable master/original imported source、capture facts、user selection、user edits/pins/confirmation、song/rules snapshot、import原文、accepted generation の由来、release receipt、必要なundo metadata。

**再生成できるが provenance は必要:** automatic boundary proposal、F0/analysis、quality observations。engine/model/rules/parameters/audio fingerprint が同じで再現できる範囲を明記する。既に人が採用・修正した generation の基底を cache と一緒に削除しない。

**evictable:** render PCM、waveform/spectrogram tiles、preprocessed audio、projection cache。byte budget・versioned key・disk GC を持ち、projectの真実ではない。

invalid numeric/import data は durable editor draft として保持できる。`ValidatedPlayableSetting` / `ExportPlan` は別で、生成時に条件を満たすことを検査する。「domain object は常に全条件を満たす」として invalid import を拒否するのは M6 仕様変更である。[R15]

## 6. State / resource lifetime と execution

```text
Application
  Library / Consent / EngineRegistry / global byte budgets / AudioHost / JobRuntime
  Project lease(epoch)
    ProjectRuntime: writer, projections, editing history, selection, project F0 statistics
    Capture lease -> continuous stream + preroll -> one Capture -> finalized Take
    Playback lease -> bounded output buffer -> native output
    Jobs(input snapshot, target rev, attempt) -> completion -> admission
    Views -> focus/filter/mode/draft/gesture session (resourcesそのものは所有しない)
```

| 実行領域 | 所有と禁止事項 |
|---|---|
| realtime callback | preallocated buffers と atomics。DB/FS/log/alloc/free/一般mutex/動的dispatch graph/await禁止。callbackの処理量はframesで有界。 |
| interactive control | AudioHostがdevice open/stop/lease管理。DBやengine完了を待たずstop要求を受けられる。OS制御の失敗を明示。 |
| durable mutation | project単位の単一writerが短い決定・CAS・SQL transaction・receiptを直列化。DSPや巨大file copyを実行しない。 |
| background computation | bounded jobs。immutable inputs、取消の観測点、CPUとbytesのadmission。native resourceは対応lane内所有。 |
| frontend observation | revision付きsnapshotと有界通知。RTメーターは独立store＋rAF、query cacheや全画面再renderに載せない。 |

AudioHost をapp-lifetimeにするのは「全viewから勝手に使える」ためではない。capture/playback lease と owner consumer が必要で、別consumerが二重録音要求を出したら Busy を返す。route cleanup が global disarm を呼んで別のownerを停止させない。録音中navigationを禁止する既存挙動は維持し、backendにも同じ拒否を置く。

SPSC handle は `Send` だが `Sync` ではなく、push/pop は排他的receiverで呼ぶ。callbackがArcの最後の参照をdropして大きな解放を起こさないよう返却/回収はcontrol側へ。出力はリングまたは事前確保chunk poolで供給し、無音は本当のunderrun時だけ。input dropoutとoutput underrunは別counterとする。

## 7. Async result の受理・cancel・retry

completion の受理条件は job の内部ではなく project owner に集約する。

```text
accept iff
  project lease is live
  AND job attempt is current
  AND relevant input revisions still match
  AND target/asset/candidate still exists
  AND cancellation has not linearized before publication
  AND protected user fields are not overwritten
```

全project revisionだけを比較すると無関係な編集でも全jobが捨てられる。`BasedOn` は asset fingerprint、rules revision、target revision、settings/selection revision 等の**その計算が実際に読むもの**に絞る。純解析の結果は有効でも、採用先だけ変わった場合、観測結果の保存とactive selectionへの適用を分ける。閉じたprojectへ旧jobを勝手に書き戻さず、再開後に有効性を確認して新attemptにする。

cancel がpublication前に確定したら、projectへの適用もcancelled job由来のcache publicationも止める。commit後のcancelは `AlreadyCommitted(receipt)` と返し、成功済みをcancelledと偽らない。FFIが走り始めたら計算自体の中断が保証できないことをAPI状態に出す。`spawn_blocking` や Drop+join を強制取消と呼ばない。[X04]

priority は既存 TR-SYN-34 を維持する。queue は job件数だけでなく入力/出力/in-flight bytesも有界。chart/reestimateはcoalesce/dropできるが、保存済みtakeの解析がqueuefullだからtakeを失敗にしない。解析待ちreceiptを返して後で投入する。長時間native call中の厳密preemptionはない。phrase/chunkなど意味を壊さない境界でyieldし、重要controlは別laneに置く。

**idempotency:** durable mutationには OperationId とrequest fingerprint、durable receiptを持つ。同じID・同じ内容は既存receipt、同じID・違う内容は conflict。返答が失われても operation status を引ける。録音のcontent hashをidempotency keyにしない。previewやhoverはdurable exactly-onceではなくrequest世代でlatest-wins。receipt retentionを超えた再送を新規操作として黙って受理しない。

shutdown は新規admission停止→lease fence→native停止/残りpump処理→必要なcommit/回復記録→job cancellation→資源回収。GUI threadでjoinしない。停止不能native callに対するhard deadlineとクラッシュ隔離が必須になった場合は別processを採るが、catch_unwindでC++segfaultが回復できるとはしない。

## 8. Application contract と技術判断

### 8.1 canonical application language

consumer-facing contract は `schema.graphql` を正本とする。GraphQL schema は Rust domain model、SQLite row、React UX model のいずれとも同一化しない。Query/Mutation/Subscriptionをそれぞれ read projection / intention / long-lived observation として使う。HTTP server、Apollo normalized cache、distributed backend は採用理由ではなく、GraphQL execution は process 内で完結する。

代表形:

```graphql
type Query {
  project(ref: ProjectRefInput!): Project
  job(id: ID!): BackgroundJob
  waveformWindow(input: WaveformWindowInput!): WaveformWindow!
}

type Mutation {
  startTake(input: StartTakeInput!): StartTakePayload!
  finishTake(input: FinishTakeInput!): FinishTakePayload!
  applyEditorCommand(input: ApplyEditorCommandInput!): ApplyEditorCommandPayload!
  cancelAnalysis(input: CancelAnalysisInput!): CancelAnalysisPayload!
  prepareExport(input: PrepareExportInput!): PrepareExportPayload!
  commitExport(input: CommitExportInput!): CommitExportPayload!
}

type Subscription {
  projectEvents(project: ProjectRefInput!, after: EventCursor): ProjectEvent!
  jobEvents(job: ID!, after: EventCursor): JobEvent!
  inputEnvelope(input: InputLeaseRefInput!): InputEnvelopeFrame!
  playbackEvents(playback: ID!): PlaybackEvent!
}
```

Durable mutation は OperationId と request fingerprint/receipt を持つ。expected application outcome は payload/union で返し、GraphQL `errors[]` を domain state machine に使わない。例えば `finishTake` は master commit と analysis readiness を分離して表す。

### 8.2 schema-first GraphQL と Rust implementation

**採用:** canonical SDL -> Rust GraphQL boundary -> frontend operations/fragments。Specta の application-contract ownership は廃止する。Specta が残る場合は OS shell/transport helper 等の application contract 外へ限定する。

Rust GraphQL tooling は code-first ergonomics が強いため、SDL -> resolver skeleton の独自generatorを作らない。`async-graphql` の runtime schema を deterministic SDL として exportし、canonical SDL と structural parity を CI で検査する。canonical SDL と全 executable documents は独立 parser/validator でも検査し、frontend codegen driftを禁止する。Rust declarationは implementation、SDLが正本である。

GraphQL adoptionのコストとして、resolver layer、schema parity、codegen、custom scalar discipline、query/subscription resource control、schema evolution reviewを正式に計上する。このコストを避けるために Rust struct を正本へ戻さない。

### 8.3 Query / coherent read session

field resolverごとにDBを読む設計は禁止する。一つのproject queryは `ProjectReadSession` を作り、ProjectLease/epoch、ProjectRevision、coherent DB/read snapshotを固定する。recording/review/repertoire/editor/distribution等のfacetは同じrevisionからlazy/materializedに解決する。GraphQL selectionの自由度が revision incoherence を生まないことを invariant とする。

巨大ProjectSnapshotを常時eager生成せず、必要facetのみ materializeする。selection ASTからSQL最適化する仕組みは性能証拠が出るまで作らない。大容量/listはbounded window/root operationにし、5000 editor entriesを無制限object graphとして常時返さない。

### 8.4 Subscription / generic Tauri transport

**すべての application-visible な継続streamをGraphQL Subscriptionへ統一する。** `Channel<Envelope>`、`Channel<JobProgress>` 等のapplication-specific Tauri contractを作らない。Tauri Channelが残る場合は `Channel<GraphqlExecutionResult>` 相当のgeneric transport detailだけ。React feature/runtimeはChannelを知らない。

Subscriptionはdurable state notification、job lifecycle、transient realtime observationを区別し、sequence/cursor、replay/resync、queue bytes、drop/coalesce policyをsourceごとに定める。Project eventのgapはQuery snapshotでresyncする。input envelope等はreplayせずlatest-wins/coalescingを許す。durable eventとtransient frameを同じdelivery policyにしない。

Observation cancellation（unsubscribe）とApplication cancellation（`cancelAnalysis` 等Mutation）を分離する。React unmountでsubscriptionを閉じてもbackground jobを自動cancelしない。RT callbackはGraphQL/serialization/Tauri IPCを実行せず、bounded non-RT observation producerの後ろでSubscription化する。

### 8.5 persisted operations / capability / evolution

Tauri command名がcapability boundaryではなくなるため、`recording.capture`, `editor.modify`, `distribution.export` 等のapplication capabilityを独立に表す。production GUI は arbitrary documentを無制限実行せず、build時に operation name/hash/kind/schema hash/required capability/cost class のmanifestを生成する。

Schema evolutionは additive change、`@deprecated`、usage-zero proof、semantic-breaking review を使う。内部Rust/DB/crate変更だけならschema変更不要。同名Mutationの意味をsilent changeしない。GUI同梱だけの期間と、MCP/extension公開後のcompatibility windowを区別する。

GraphQLの詳細設計と採否比較は `08-graphql-application-contract.md` を正本候補とする。

### 8.6 Composition / DI

```rust
// 構成意図を示す擬似コード。
let projects = ProjectStorage::new(library_root);
let audio = AudioHost::start(PlatformAudio::detect(), rt_budget)?;
let engines = EngineServices::load_available(models, engine_budget)?;
let jobs = JobRuntime::start(engines, job_budget);
let app = Application::start(projects, audio.control(), jobs, consent_store);
let schema = GraphqlSchema::new(app);
let desktop = TauriGraphqlTransport::new(schema);
```

pure functionにDIは不要。resolverへrepository/audio/worker traitsを個別injectせず、Application/ReadSessionだけを渡す。SQLite storeはまずconcreteでreal temp DBをtestする。native handlesはowner threadへmoveし、各use caseへArcで無制限共有しない。runtime polymorphismは実際に複数backend/engineがある境界に限定する。

## 9. React の ownership

```text
ui/src/
  app/                       routes, project lease, operation composition
  application/graphql/       generic execute/subscribe transport, TanStack adapter, generated artifacts
  features/
    recording/               fragments + interaction
    take-inspection/         fragments + review interaction
    editor/                  fragments + draft/gesture/table/undo + wasm facade
    song-planning/           fragments + song selection/range UI
    distribution/            fragments + metadata/export intent
  ui/                        reusable accessible primitives, no domain imports
```

中央のdomain-specific `queries.ts` を長期ownerにしない。各feature/componentは必要なapplication model断面をGraphQL fragmentとして近傍に置く。feature Aがfeature Bのprivate query hookをimportせず、両方がcanonical schemaに依存する。route/workflow composition boundaryがchild fragmentsを一つのoperationへ合成する。fragment colocationはcomponentごとにnetwork/IPC requestを発行することを意味しない。

```text
data dependency ownership = component / feature
operation/request ownership = route / workflow composition boundary
```

local stateはfocus、selection、expanded panels、pure ViewMode、gesture。durable/read stateはGraphQL Query。EditingSessionのnormal/advanced operation modeはapplication contractで観測/変更する。RT observationはGraphQL Subscriptionをfeature-local external storeへ流し、rAF/useSyncExternalStore等で描画する。録音resourceはReact effect自体に所有させない。

**TanStack Query は維持するが normalized GraphQL cache は導入前提にしない。** Query/Mutation operation resultのloading/error/retry/suspense/invalidationを担当する。keyは概ね `[operationHash, variables, projectLease]`。revisionごとの無限keyを作らない。projectEvents/MutationReceiptでactive operationをinvalidateし、最初はproject scope refreshを優先、実測後にchangedFacets等で細粒度化する。

High-frequency Subscriptionを毎frame query cacheへ書かない。durable change notificationはQuery refresh、meter/envelope/playback position等はfeature-local transient storeへ。Subscriptionのunsubscribeをjob cancelと解釈しない。

Suspenseは独立boundaryのloadingに限定する。同時に必要なdataはroute operationでfragment compositionし、連続Suspense hookのwaterfallを作らない。

TanStack StartはGraphQL判断と独立である。productionでSSR/serverを使わずdev互換設定だけが残るなら Vite + TanStack Router client SPAへの簡素化を引き続き候補とするが、GraphQL migrationと同じPRで不要なframework置換をしない。

### 9.1 M6 の8msと規則重複の解決

ドラッグの各frameをTauri往復にしない。同じ `model::editing` kernelをWASMで呼び、sample整数・operation種別・modifier・snap candidates・base stateからproposalとviolationsを得る。Reactはpixels↔sampleの表示変換とpointer/focusだけを扱う。確定時はnative側が同じkernelと最新revisionで検証し、単一undo単位でcommitする。WASMが真実を持たず、DB/write/selectionはできない。

追加build targetという欠点は認める。まず3ms制約・snap・negative overlap・IME・drag性能を含む代表sliceでnative/WASM parityを試す。WASMが不利ならローカルdraftは単なるraw proposalに限定し、承認済み状態を偽らない代案を評価するが、最終仕様の常時制約/8msを削って帳尻を合わせない。

## 10. Filesystem durability / DB / migration

### 10.1 Capture commit protocol

1. CaptureIdを一意に確保する。必要ならCaptureIntentを先に永続化するが、これはtakeではない。既存FSL designの「DBより先」の詳細化変更は別途承認する。
2. 同じfilesystem内に `create_new` でpartを作る。既存fileをtruncateしない。audioをstreamし、metadata/headerをfinalizeする。
3. fileのflush/syncを検査する。no-replace final renameとparent directory durabilityをplatform実装で保証する。単なるrename成功をpower-loss耐性と言わない。
4. sealed FinalizedAssetを得た後にDB transactionでtake、capture outcome、要件に沿う選択、operation receipt、revisionを確定する。
5. 解析・F0・FRQ・alignment・renderは別job。保存成功後の解析失敗はtakeを消さず、retry可能なanalysis状態にする。

| fault point | 再起動時の状態と行動 |
|---|---|
| part書込中 / file sync失敗 | take rowなし。部分音声の扱いは既存契約に従う。自動的に有効takeへ昇格しない。 |
| rename失敗 | partを保持/報告、take commitなし。同じ名を上書きしない。 |
| rename後、directory sync不確定 | durableだと断定しない。recovery scan対象。 |
| final durable後、DB commit前 | orphan finalized WAV。本人へ復旧候補を提示し、本人の判断まで消さない。 |
| DB commit後、応答前 |同OperationIdで同receiptを返す。二重takeを作らない。 |
| commit後のanalysis失敗 | takeは存在、derived stateはfailed/pending。録り直しを自動要求しない。 |

filesystemのsupported範囲、Windows sharing/rename、flushのOS意味、disk/hardware制約をadapter contractで明示する。DB transactionとfile commitを一つのatomic operationとは呼ばない。[R09,R10,X03]

### 10.2 Snapshot / export / migration

snapshotはSQLite backup API等による一貫したDB snapshot＋そのrevisionが参照するimmutable asset manifest/hashから作る。必要な短いwriter barrier/asset leaseを保持し、copy中のGCやselected generation差替を防ぐ。nested tone assetsもmanifestから追い、直下directory walkに頼らない。snapshot自体のfilesをsyncし検証してからpublishする。[R09,X03]

exportはdry-runでcurrent filesをrenameしない。immutable planを固定し、temporary outputに生成→validate/readback→flush/publish→release receipt。出力済み・receipt未commitの途中状態はreconciliationし、同operationのretryで二重releaseにしない。原projectの音声やaliasを配布表現のために破壊的変更しない。

migrationはsupported旧versionごとのfixtureを揃え、sourceをconsistent backup後、staging generationへ変換する。assetsはbyte hashで保護、ID mappingを明示、raw OTOやpins/confirmation/selected takeを保存し、未知enum/破損を既定値に丸めない。検査が全て通った後にgeneration selectorを原子的に切り替える。途中で落ちた場合は旧generationを開けるか、明示recovery状態にする。旧backupはread-only保管であり、旧runtimeをproductionで並行運転する意味ではない。切替後は新構造だけにwriteする。

manifestとDBに同じmutable truthを二重管理しない。manifestはformat/bootstrap locator、DBはprojectのmutable truthとする案を推す。既存FSL/互換形式に触る変更はDECとmigrationに明記する。

## 11. M6 / M7 / その先

M6は同じEntryId/command kernelにnormal/advanced、numeric invaliddraft、manual protection、12種＋制限式bulk、lossless import、external watcher、undo、bounded chartsを載せられる。操作modeの切替でdata modelを変えない。先行発声確認/合成試聴は本体capabilityでありpluginへ追い出さない。[R15]

M7の3OS実装はAudioHost backend、Tauri OS integration、packaging/signing/update adapterで増やす。未対応を capability unavailable と返すだけではM7完了ではない。platform native/IME/a11y/real-device/release tests が必要。ConsentはApplication owner、telemetry/crashreport別、送信直前にもgrantを検査する。既定off・SaaS・no self-hostを保持し、未決定SaaSを勝手に決めない。[R16,R17]

将来MCP/automation/extensionは原則canonical GraphQL executorのapproved operationを使う。GUIだけがGraphQL、MCPだけがRust Application APIを直接叩く二重consumer contractは作らない。consumerごとの差はauthorization、file scopes、user confirmation、capability、rate/resource admission、transportである。並行processが同じprojectを書かないようwriter lease/lockを守る。任意pluginのABI/sandbox/permissionsはまだ作らない。trusted adapterとuntrusted extensionは同じではなく、後者には別process/sandbox decisionが残る。

## 12. 最大の欠点、選ぶ理由、人の決定

最大の欠点は、**identity migration と result admission を一度に正しく導入する難しさ**、そしてM6のnative/WASM二targetの保守である。新しいProjectRuntimeが巨大化する危険も残る。この費用を払う理由は、現在既に起きたalias取り違え・partial commit・false greenと、M6の手編集競合を同じ少数の境界で説明できるからである。将来の流行を予想しているからではない。

human/product reviewが必要な事項:

- custom presamp/rulesの外部変更時の再解釈、旧targetとのequivalence、候補再選択。
- historical manual pinsを新takeへ移す正確なpolicyと曖昧な旧identity migration。
- imported stereo/rate/bit-depthの受入・変換、WAV長変更のanchor選択。
- zero-cross/pitchmark詳細、macOS keyboard/IME実現性、normal-mode correction品質の受入。
- priority下での実測budget、supportするfilesystem/OS/device、native不具合のprocess isolationが必要な度合い。
- telemetry SaaS、保持期間・所在地・送信fields、モデル/データのlicensing判断。
- product completion / coverage表示とexport eligibilityの意味。都合のよい一つのpercentageへ合成しない。

要件のconfidenceがAssumptionであっても勝手に削除しない。accepted behaviorを変更するなら、根拠・代替・human approvalを伴うsuperseding DECにする。

## 13. 完成条件と質問への索引

architecture redesignは図をmergeした時点では終わらない。canonical ruleとcanonical SDLが登録され、runtime-exported schemaとのparity、全operation/fragment validation、ownersとprivate/public boundary、risk-based testの実行証跡、GraphQL Query/Mutation/Subscriptionと診断signalの整合、application-specific Specta/Tauri command/channel surfaceの除去、contributor/Skillの正本参照まで成立した時点を完成とする。

| 質問番号 | 主な回答先 |
|---|---|
| 1–3 現状・境界・変更圧力 | 本書1–3、evidence |
| 4–10 Rust/React/API/lifetime/identity/DI/query | 本書4–9 |
| 11–12 concurrency/durability | 本書7・10、tests-observability |
| 13–17 test portfolio/実行/falsegreen/performance | tests-observability 1–5、task DAG |
| 18–23 errors/tracing/metrics/privacy | tests-observability 6–10 |
| 24–25 M6/M7/futureconsumer | 本書11、worked examples |
| 26–29 contributor判断/enforcement | contributor guide、tests-observability |
| 30–31 Skills/handoff | skills-design、agent-handoff |
| 32–35 欠点・理由・DAG・human decisions | 本書12、task DAG |

## 14. 実装時に曖昧にしない補足契約

### Project lease と recording session は同じではない

現段階ではmutableなactive ProjectRuntimeは一つでよい。追加consumerが同じprojectへattachしても、DB writerや録音sessionを二重に作らない。別projectへの切替は明示operationにし、録音中や他のresource leaseが保護している場合はBusy/confirmation policyで扱う。複数projectを同時編集するplatformを先回りして作らない。

`ProjectEpoch` はclose/reopenで変わる。viewのmount/unmountや同projectへのconsumer attachだけで、収録条件snapshotを持つ `RecordingSessionId` を作り直さない。`InputLease` は連続入力/prerollを所有し、`CaptureId` はそのstreamで録る一回、`TakeId` は確定master。各takeは実際に使ったsession/settings snapshotを参照する。epoch、session、stream、capture、takeの五つを一つのIDへ畳まない。

### Native-free schema / contract validation と feature discipline

canonical SDL と frontend operation codegen は native engine を構築せず検査できるようにする。`koeru-graphql` の schema/type layer と runtime application interfaces は native adapters の compile/link を要求しない構成を優先する。public GraphQL shapeをplatform featureで変えず、capability unavailable は同じschema内のtyped outcome/stateで表す。

full production構成がexportするruntime SDLとcanonical SDLのparityをCIで検査し、portable contract-check構成でもcanonical SDL/operationsをparse/validateできるようにする。native未構築時に各操作を偽成功させるstubは作らない。これはcontract validationとportable logicのcompile isolationのためで、native testsを0件にして通すためのfeatureではない。

### Engine resources / binary contracts

重いmodelをworker数だけ複製しない。FFIがthread-safeな共有read-only modelを保証する場合だけ共有し、そうでなければengine laneが所有して直列に使う。model residency、worker scratch、native/WASM/JS間copyはすべてmemory budgetに算入する。single-writerだけではlatencyが保証されないため、重いcopy/DSPをwriterから外し、durable commit待ち中でもAudioHostのstopとread済みprojectionの表示を妨げない。

wire上のID/epoch/revisionはopaqueな値とし、JSの数値精度限界を越えるintegerを無検査でnumber化しない。sample位置は対象asset/timebaseと入力上限を検証する。binary resourceはlength/rate/format/checksumとchunk boundaryを持ち、client cacheが勝手に別assetへ流用しない。

### UIの表示modeとFSLの操作modeを混同しない

focus/zoom/layoutだけのViewModeはfrontend-localでよい。しかしFSLは「通常modeでadvanced_edit/無制約編集を行えない」「close advancedでunconstrainedを解除する」を要求する。[R26] この制約はUIでボタンを隠すだけでは守れない。

そこでconsumer/project leaseに属するephemeral `EditingSession` が操作modeとsession revisionを所有する。open/close advancedはsession transitionであり、entry・project durable data・undo historyは変更しない。UIはそのprojectionを表示する。kernelとnative apply-editはEditingSessionのmodeを確認する。WASMは同じsession snapshotでpreviewするだけで、閉じたadvanced sessionの遅延commandは拒否する。GUI以外のconsumerも明示的にediting sessionを開始/遷移して利用できる。

このinteraction modeと、memory allocation用のrecording/editing activity mode、純粋な画面layoutは別の目的を持つ。すべてを一つのglobal mode enumへ詰め込まない。