# GraphQL Application Contract — canonical SDL / in-process execution / generic Tauri transport

**状態: architecture target。GraphQL を network API としてではなく、KOERU application が consumer に約束する language / capability の正本として使う。**

本書は `00-architecture-report.md` の application contract 判断を詳細化する。FSL/meta が product semantics の正本であり、本書や SDL が product rule を再定義してはならない。GraphQL SDL は **consumer-facing application contract の正本**である。

## 1. 採用理由

現在の Specta 系は Rust type/function から TypeScript を生成し、Rust/TS の型ずれ防止には強い。一方、Rust command registry と DTO が contract の原型になりやすい。M6/M7 では recording/editor/review/song/audition/distribution/background analysis/project/derived/user-edited state を複数 feature が異なる projection で読む。さらに MCP/automation/extension の追加可能性がある。

必要なのは `Rust implementation -> TypeScript` の同期ではなく、次の三者を分離すること。

```text
Rust Domain / Runtime Model
          !=
Canonical GraphQL Application Contract
          !=
React UX / Component Model
```

GraphQL の採用理由は CRUD、HTTP、distributed system、network efficiency ではない。次を既存言語として得ることにある。

- `Query`: coherent application state の read projection。
- `Mutation`: user intention / application use case。
- `Subscription`: long-lived state transition / background operation / transient observation。
- fragment/selection: consumer/feature が必要な application model の断面を近傍で宣言する仕組み。
- SDL: Rust/React のどちらにも所有されない canonical consumer contract。

## 2. 正本の階層

```text
specs/ (FSL) + meta/
  product state/transitions/invariants/requirements/decisions
                    |
                    v
specs/application/schema.graphql   <- canonical consumer contract
                    |
        +-----------+-----------+
        |           |           |
      Query      Mutation   Subscription
        |           |           |
        +------ koeru-graphql ---+
                    |
               Application
                    |
        ProjectRuntime / jobs / resources
```

FSL/meta の命題を SDL description に複製しすぎない。SDL は vocabulary、shape、capability、expected outcome、evolution を表す。例えば「confirmed boundary を再推定で上書きしない」という policy は model/FSL が所有し、SDL は confirmation/provenance/outcome を consumer に観測・操作できる形で表す。

## 3. crate / dependency boundary

目標依存方向:

```text
koeru-model / formats / audio / align / synth
                    ^
                    |
              koeru-runtime
                    ^
                    |
              koeru-graphql
                    ^
                    |
              koeru-desktop
                    |
                  React
```

`koeru-graphql` は GraphQL root/type/scalar/adapter と schema parity だけを所有する。SQLite/Diesel、native audio、filesystem protocol、engine implementation、business policyを直接知らない。resolver は Application/ReadSession を呼ぶ。

禁止:

```text
koeru-model   -> async-graphql
koeru-runtime -> async-graphql
koeru-audio   -> async-graphql
feature/*     -> Tauri invoke / Channel
```

Domain struct に `SimpleObject` 等を直接 derive しない。Diesel row、domain object、GraphQL object、React props は同一化しない。

## 4. schema-first を Rust tooling の弱さ込みで成立させる

Rust の主流 GraphQL server library は Rust type/macro から static schema を構築する code-first ergonomics が強い。したがって SDL から Rust resolver skeleton を全面生成する独自 framework は作らない。

採用する enforcement:

```text
canonical schema.graphql
      |                \
      |                 -> frontend documents/codegen
      v
parse / validate
      ^
      |
async-graphql runtime schema --sdl() / sdl_with_options()
```

CI は canonical SDL と runtime-exported SDL の structural parity を検査する。Rust declaration は implementation であって正本ではない。operation/fragment は canonical SDL に対して validation し、生成 TS は drift-free を要求する。

この二重記述コストは GraphQL 案の実コストとして受け入れる。schema-first tooling が十分成熟した場合のみ generator を差し替え、application semanticsを変えない。

## 5. Query: coherent projection

GraphQL field resolver ごとに DB を引かない。Query operation は最初に project-scoped coherent read context を作る。

```text
GraphQL operation
      |
      v
ProjectReadSession
  ProjectLease / ProjectEpoch
  ProjectRevision
  coherent SQLite/read snapshot
  lazy/materialized facets
      |
      +-> recording()
      +-> review()
      +-> repertoire()
      +-> editor()
      +-> distribution()
```

一つの query response に含まれる project-derived fields は同じ revision を観測する。`recording` が R42、`review` が R43 になることを禁止する。巨大 `ProjectSnapshot` を毎回 eager 構築せず、同一 revision の lazy facets を materialize する。

例:

```graphql
type Query {
  project(ref: ProjectRefInput!): Project
  job(id: ID!): BackgroundJob
  waveformWindow(input: WaveformWindowInput!): WaveformWindow!
  spectrogramWindow(input: SpectrogramWindowInput!): SpectrogramWindow!
}
```

高コスト・大容量 field を arbitrary deep traversal にせず、明示 root operation / bounded window として置く。GraphQL selection AST から SQL optimizer を作ることは初期要件ではない。

## 6. Mutation: intention / receipt / expected outcome

DB CRUD を expose しない。Mutation 名は application use case とする。

```graphql
type Mutation {
  startTake(input: StartTakeInput!): StartTakePayload!
  finishTake(input: FinishTakeInput!): FinishTakePayload!
  applyEditorCommand(input: ApplyEditorCommandInput!): ApplyEditorCommandPayload!
  cancelAnalysis(input: CancelAnalysisInput!): CancelAnalysisPayload!
  prepareExport(input: PrepareExportInput!): PrepareExportPayload!
  commitExport(input: CommitExportInput!): CommitExportPayload!
}
```

Durable mutation は `operationId` を受け、同じID・同じrequestは同じreceipt、同じID・違うrequestは conflict とする。

Expected application failure を GraphQL execution error にしない。

```graphql
type FinishTakePayload {
  receipt: MutationReceipt!
  outcome: FinishTakeOutcome!
}

union FinishTakeOutcome =
    TakeCommitted
  | FinishTakeRejected
  | FinishTakeConflict
  | RecoveryRequired
```

`TakeCommitted` は analysis が pending/failed でも durable master commit を明示できる。GraphQL `errors[]` は invalid document、resolver bug、unexpected invariant failure 等に限定する。

## 7. Subscription: application-visible stream を全面統一

**Rust→React の application-visible な継続ストリームはすべて GraphQL Subscription とする。** application-specific `Channel<Envelope>` / `Channel<JobProgress>` / `Channel<ProjectEvent>` は禁止する。

代表:

```graphql
type Subscription {
  projectEvents(project: ProjectRefInput!, after: EventCursor): ProjectEvent!
  jobEvents(job: ID!, after: EventCursor): JobEvent!
  inputEnvelope(input: InputLeaseRefInput!): InputEnvelopeFrame!
  playbackEvents(playback: ID!): PlaybackEvent!
}
```

Subscription source は lifetime owner と対応する。

| Subscription | owner | delivery semantics |
|---|---|---|
| projectEvents | ProjectRuntime | durable state-change notification。gapならQuery snapshotでresync |
| jobEvents | JobRuntime/job | lifecycle/progress。現在stateをQueryでも取得可能 |
| inputEnvelope | InputLease observation producer | transient、bounded、latest-wins/coalesce可、replayなし |
| playbackEvents | PlaybackLease | transient/status、bounded、replayなし |

GraphQL subscription root は一 operation 一 root field とする。巨大 `everythingEvents` を作らず resource/lifetime owner ごとに分ける。

### 7.1 Tauri Channel の扱い

Tauri Channel がコードに残る場合でも、それは generic transport detail のみ。

```text
GraphQL execute_stream()
        |
        v
Channel<GraphqlExecutionResult>
        |
        v
React GraphQL transport client
```

Tauri transport は概念的に `execute`, `subscribe`, `unsubscribe` のgeneric operationのみ。React feature は Tauri Channel を importしない。runtime/ApplicationはTauriを知らない。

巨大 binary payload が将来ボトルネックなら binary resource transport を最適化してよいが、lifecycle/identity/sequence/completion/failure は Subscription contract の所有に残す。最適化を理由に application-specific Channel contract を復活させない。

### 7.2 cancellation

二つを分離する。

- observation cancellation: unsubscribe。「もう観測しない」。job/capture自体を止めない。
- application cancellation: `cancelAnalysis`, `stopPreview` 等のMutation。「作業を止めたい」。

React unmount -> unsubscribe が background job cancel を意味してはならない。

### 7.3 sequence / replay / backpressure

Subscription payload は必要な stream で sequence/cursor を持つ。durable event の gap は Query refresh、transient frame の gap は dropped observation として扱う。

各 source は replay/delivery policy を宣言する。

```text
project events: resyncable, loss-intolerant notification
job lifecycle: current-state + subsequent / bounded history as needed
meter/envelope: non-replay, loss-tolerant, coalescible
```

queue policy を一つに統一しない。durable event は落とさず、20Hz meter は詰まれば古い観測を捨てる。GraphQL は backpressure を自動解決しないため resource owner が item/bytes budget を持つ。

## 8. React ownership / fragment colocation

中央の domain-specific query factory を廃止方向にする。

```text
ui/src/
  app/                    route/operation composition, project lease
  application/graphql/    generic transport, TanStack adapter, generated artifacts
  features/
    recording/
      RecordingHeader.tsx
      RecordingHeader.graphql
    editor/
      EditorWorkspace.tsx
      EditorWorkspace.graphql
    review/
      ReviewQueue.tsx
      ReviewQueue.graphql
```

Component/feature は必要な断面を fragment で宣言する。

```graphql
fragment RecordingHeader_Project on Project {
  revision
  recording {
    progress { covered required }
    activeTake { id state }
  }
}
```

route/feature composition boundary が child fragments を一つの operation に合成する。fragment colocation は component ごとの network request を意味しない。

```text
data dependency ownership = component/feature
operation/request ownership = route/workflow composition boundary
```

feature A は feature B の private hook をimportせず、両者が canonical schema に依存する。

## 9. TanStack Query と transient stores

Apollo normalized cache は採用前提にしない。TanStack Query を operation result lifecycle に使う。

Query key の基本:

```text
[operationHash, variables, projectLease]
```

revisionを無限keyにしない。Mutation receipt / projectEvents で active project operations を invalidate/refetch する。最初は project-scope invalidation を優先し、performance evidence が出てから `changedFacets` 等で細粒度化する。

High-frequency Subscription は query cache に毎frame書かない。

```text
durable query state        -> TanStack Query
durable change notification -> invalidate/refetch
transient observation       -> feature-local external store + rAF/useSyncExternalStore
```

## 10. M6 editor

GraphQL 採用でも per-pointermove IPC はしない。WASM editing kernel を維持する。

```text
pointer move
 -> local WASM kernel proposal
 -> React visual draft

pointer release
 -> GraphQL Mutation applyEditorCommand
 -> native same kernel + latest revision validation
 -> durable one-undo commit
```

Editor entry list は5000件規模を想定し、GraphQL object graph 全件・全fieldを無制限に取らない。UI visualization 用に bounded window/range projectionを設計し、bulk edit自体はApplication側で対象集合へ作用する。visualization needs と bulk computation needsを分ける。

Resource を作る preview は Query としない。CPU/queue reservationを伴うなら `start*` Mutation + job Subscription/Query statusにする。pure read-only calculationならQueryとしてよい。

## 11. capability / persisted operations / complexity

Tauri command 名が capability boundary ではなくなるため、application capability を独立に表す。

例:

```text
recording.read
recording.capture
editor.read
editor.modify
analysis.cancel
distribution.prepare
distribution.export
```

当初 desktop GUI は許可された全 desktop-user capability を持ってよい。future consumer では operation manifest と policy によりsubsetを与える。

Production GUI は arbitrary GraphQL text の実行を必要としない。build時に operation manifest を生成する。

```text
operation name
normalized document hash
kind
schema/contract hash
consumer
required capabilities
cost class
```

runtime は approved operation + variables を実行する。これにより query complexity、deprecated usage、capability、supportabilityを operation 単位で管理する。

Generic max-depthだけに依存せず、`cheap-read`, `large-read`, `compute-heavy`, `job-creating`, `native-resource` 等の application cost class と実測budgetを持つ。

## 12. host shell boundary

GraphQL を「Tauri の全機能を置換するもの」にしない。file picker、window、OS reveal 等の host-shell action は application semantics とは別境界。

```text
React
  -> GraphQL: KOERU application
  -> desktop host API: OS shell/window/dialog
```

ただし host で選ばれた raw path を万能権限として application contract へ流さない。import/export は semantic `ImportSource` / `ExportDestination` 等へ変換する方針を検討する。

## 13. future MCP / automation / extension

外部 consumer は Application Rust API を直接別contractとして使わず、原則 canonical GraphQL executor を通す。

```text
MCP tool / automation adapter / trusted extension adapter
             |
       approved GraphQL operation
             |
       same GraphQL executor
             |
          Application
```

consumer差は authorization、file scope、confirmation、resource quota、transport。business/application capabilityを再実装しない。

Untrusted plugin の process isolation/ABI/sandbox は future decision。GraphQL adoptionを理由に今generic plugin frameworkを作らない。

## 14. schema evolution

- internal Rust/DB/crate changeだけなら schema change不要。
- output field追加は原則additive。
- renameは新field追加 -> old `@deprecated` -> usage zero証明 -> removal。
- mutation semanticsを同名のままsilent changeしない。必要ならnew input mode/new mutation/new field。
- identity semantic changeはmigration/DEC/review対象。
- GUI同梱consumerだけの間はcompatibility windowを短くできるが、MCP/extension正式公開後はdeprecation policyを長期化する。

`ApplicationInfo` 等に application version と contract/schema hash を露出して support bundle と execution receipt に結び付ける。

## 15. error model

Expected outcome は payload union/object。GraphQL `errors[]` を domain state machine に使わない。nullable propagationで「analysis failed」等の正常に説明可能な状態を表現しない。

```graphql
union AnalysisState =
    AnalysisReady
  | AnalysisPending
  | AnalysisFailed
  | AnalysisUnavailable
```

既存の public error taxonomy（invalid input / rejection / conflict / cancellation / already committed / backpressure / device / storage / corruption / unsupported / native failure / invariant bug）は維持し、GraphQL representation を expected outcome と unexpected execution error に分離する。

## 16. privacy / observability

operation名/hash/kind、safe IDs、counts、duration、response bytes、subscription lifecycle を観測する。GraphQL document本文とvariables全文をnormal trace/telemetryへ出さない。variablesにはpath/lyrics/alias/title等が入り得る。

追加signal:

- query snapshot construction latency
- GraphQL execution latency/overhead
- response bytes
- active subscriptions
- subscription queue bytes
- sent/coalesced/dropped observation count
- cursor gap/resync count
- operation manifest/schema hash

RT callback はGraphQL/serialization/tracingを実行しない。RT→bounded observation→non-RT GraphQL adapterの順を守る。

## 17. mechanical fitness functions

最低限:

```text
Cargo: model/runtime/audio -> async-graphql dependency forbidden
Rust: resolver -> storage/native crate direct dependency forbidden
SDL: canonical SDL == runtime-exported schema structurally
Documents: every operation/fragment validates against canonical SDL
Frontend: feature imports Tauri invoke/Channel forbidden
Transport: application-specific Channel<T> forbidden
Operations: production operation manifest complete
Privacy: GraphQL text/variables cannot enter normal trace/telemetry constructors
Query: project-derived response is one coherent revision
Subscription: required streams have sequence/delivery/backpressure policy
Evolution: deprecated field cannot be removed while operation usage > 0
```

Guard は positive/negative canary と execution receiptを持つ。operation/fragment discovery 0件を green にしない。

## 18. adoption sequence

全面置換から始めない。代表 vertical slice で architecture assumption を検証する。

1. canonical SDL skeleton と parity/validation tooling。
2. cross-cutting `Project` Query + coherent ProjectReadSession。
3. `finishTake` Mutation: durable receipt / partial success / expected outcome。
4. `jobEvents` + `inputEnvelope` Subscription: lifecycle と transient stream の両方。
5. React fragment colocation + TanStack integration。
6. M6 editor representative fragment/mutation + WASM commit。
7. remaining IPC/Channel migration。
8. application-specific Specta command surfaceを削除。

GraphQL layer が第二の Studio/God API になった場合は adoption失敗と判定する。schema field数ではなく vocabulary/cohesion/consumer dependency をreviewする。

## 19. 採否の再判断

GraphQL の価値を不要と判断したのではない。schema-first Protobuf等でも application contract independence は作れるが、frontend feature-local projectionを同等に強くするには typed FieldMask、selection composition、result narrowing、dependency manifest等を追加実装する必要がある。

KOERUでは M6 時点で異なる feature が同じ application model の異なる断面を読むため、fragment/selection が architecture mechanismとして働く。Rust schema-first tooling、resolver layer、schema/codegen/evolution discipline、query/subscription resource controlのコストを払っても、**canonical application language + feature-local data dependency + future consumer reuse を一つの既存contract systemで統一する価値が上回る**と判断する。
