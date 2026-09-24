# KOERU Architecture Refactoring — Agent Task DAG (GraphQL contract revision)

## 0. Delivery model

最終的に単一の新runtimeと単一のconsumer contractへ切り替える。productionで旧Specta/Tauri application surfaceと新GraphQL surfaceを恒久併用しない。旧data reader/migration fixture/read-only backupは互換・復旧のため残してよいが、旧write pathとapplication-specific Channel contractを残さない。

T00–T17がarchitecture refactoring deliverables。H6/H7はその後のmilestone completion branch。T13でM6の難しい代表sliceを実装・検証し、GraphQL/WASM/read modelが実要件へ耐えることを確認する。T16のcutoverまでは既存機能の等価性とsafe rollback pathを維持する。

```text
T00 -> T01 -> T02 -> T03 canonical SDL
                |       |\
                |       | +-> T04 storage ------+
                |       | +-> T06 audio -----+  |
                |       | +-> T07 jobs ----+ |  |
                |       v                  | |  |
                +-----> T05 ProjectRuntime/read session
                           |              | |  |
T02 + T04 + T06 + T07 ---> T08 engines/import/export
T03 + T05 + T07 ----------> T09 GraphQL adapter
T03 + T09 ----------------> T10 generic Tauri GraphQL transport/ops manifest
T03 + T05 + T09 + T10 ---> T11 frontend fragments/TanStack
T06 + T07 + T09 + T10 ---> T12 Subscription unification
T02 + T04 + T05 + T11 ---> T13 M6 editor vertical slice
T03 + T05 + T07 + T09 ---> T14 diagnostics/errors/consent/privacy
T04..T14 ----------------> T15 integrated fitness/performance/schema gates
T15 ---------------------> T16 coherent cutover / legacy contract removal
T16 ---------------------> T17 contributor governance / Agent Skills
T16 ---------------------> H6 / H7 milestone completion
```

公開contract/root configを複数agentが独自変更しない。T03はcanonical SDL vocabulary、T04はstorage/migration、T05はProjectRuntime/read consistency、T06はRT unsafe、T07はjob/result admission、T09はGraphQL Rust adapter、T10はdesktop transport/persisted operations、T15はCI/fitness portfolioのowner。interface変更要求は該当ownerへ返す。

各task cardは Objective / Prerequisites / Architecture contract / Owned or affected / Must not change / Tests / Observability / Migration or recovery / Acceptance / Adversarial / Verification / Evidence outputs / Human review or decision の13項目を持つ。

## T00 — Baseline / verified finding ledger

- **Objective:** 実merge予定SHA、open PR、CI状態、既存contract surfaceを固定し、事実・記録・推論・未実行を分ける。
- **Prerequisites:** なし。
- **Architecture contract:** existing semanticsを変更しない。GraphQL adoptionを既存動作の証拠として扱わない。
- **Owned / affected:** evidence registry、CI receipts、current Specta/Tauri command/channel inventory、query factory inventory。
- **Must not change:** privacy guardを緩めてgreen化、test skip、review commentを全てcurrent defect扱い。
- **Tests:** current FSL/meta/license/native/UI/unsupported checks、current binding generation、Channel/command discovery、alias/snapshot/SPSC最小再現。
- **Observability:** existing trace fields/events/metrics出口のinventory。安全なcount field修正だけ必要なら別small patch。
- **Migration / recovery:** schema/dataを書き換えない。legacy project fixture候補をinventory。
- **Acceptance:** tested SHA、actual command/query/channel surface、実行/未実行/失敗が明確。
- **Adversarial:** 0 tests、missing fixture/LFS、wrong cfg、stale bot review、generated filesだけ見て実装を誤認。
- **Verification:** clean checkout/connector evidenceで再現可能なbaseline receipt。
- **Evidence outputs:** baseline.json、surface inventory、finding ledger、CI/log refs。
- **Human review / decision:** native/DSP/perceptual未実施を完了扱いしない。

## T01 — Canonical ownership / semantic decisions

- **Objective:** invariant owner、identity、partial-success、stale/cancel、privacy/test obligationを採択可能なcontractにする。
- **Prerequisites:** T00。
- **Architecture contract:** FSL/metaがproduct semanticsの正本。GraphQL SDLはconsumer vocabularyの正本でありFSL/metaを上書きしない。
- **Owned / affected:** DEC/Q/FSL refinement、architecture index、owner map、test portfolio skeleton、consumer-contract decision。
- **Must not change:** fixed tone/transpose/normal-mode semantics、orphan手動復旧、consent sequence等をarchitecture都合で変更しない。
- **Tests:** FSL chain/refinement、meta refs/index、owner config validation。
- **Observability:** stable failure/event/privacy categoriesの必要集合を定義。
- **Migration / recovery:** CaptureIntent/new identity/migration ambiguityをdecision list化。
- **Acceptance:** 各invariantにdecision owner/commit owner/resource owner/test obligationがある。GraphQL adoption decisionがaccepted/pendingとして記録される。
- **Adversarial:** same target別representation、A→B→A、protected edit、file-only commit、unsubscribe≠cancel。
- **Verification:** current code counterexampleとtarget owner mapを並べてreview。
- **Evidence outputs:** decision list、owner map、open human questions。
- **Human review / decision:** semantic equivalence、pin transfer、GraphQL canonical-source decision、host-vs-application boundary。

## T02 — Pure identity / editing / material kernel

- **Objective:** IO/native/consumer contractから独立したsemantic kernelを作る。
- **Prerequisites:** T01。
- **Architecture contract:** row/target/capture/take/candidate/entry/selectionを分離。aliasはidentityでない。invalid draftとvalidated/playable/exportableを分離。
- **Owned / affected:** `koeru-model`、formatsのsemantic-independent AST、既存policy移設。
- **Must not change:** phonemizer互換、manual/confirmed protection、undo semantics、lossless import requirements。
- **Tests:** property/metamorphic、selection uniqueness、repacked rows、edit/inverse、protected fields、invalid numeric/import。
- **Observability:** pure logicはloggingせずtyped decision reason/violationを返す。
- **Migration / recovery:** stable mapping helperとambiguous mapping resultをT04へ提供。
- **Acceptance:** no SQLite/Tauri/GraphQL/native dependencies。preview/exportが同じ resolver/kernel を使える。
- **Adversarial:** duplicate alias別tone、same take複数span、repeated mora、rename、negative overlap、historical vs adopted。
- **Verification:** native-free build/property seeds/current goldens。
- **Evidence outputs:** semantic contracts、fixtures、dependency report。
- **Human review / decision:** retake manual intent transfer、custom rules equivalence。

## T03 — Canonical GraphQL SDL / application vocabulary

- **Objective:** Rust/Reactのどちらにも所有されない consumer-facing application language を定義する。
- **Prerequisites:** T01,T02。
- **Architecture contract:** canonical `schema.graphql`; Query=read projection、Mutation=intention、Subscription=evolving observation。DB CRUD/Studio methodsを写さない。
- **Owned / affected:** SDL、scalar policy、naming/evolution policy、capability vocabulary、expected outcome shapes。
- **Must not change:** SDLをFSLの代替仕様にする、Rust domain objectを1:1 export、HTTP/server/Apollo cacheを採用理由にする。
- **Tests:** SDL parse/validate、schema lint、representative documents、breaking/deprecation fixtures。
- **Observability:** operation name/hash/kind、schema hashの安全なfieldだけ定義。variables/document本文をlogging対象にしない。
- **Migration / recovery:** initial contract hash/version policy。legacy Tauri surfaceとの移行表だけ作る。
- **Acceptance:** recording/editor/review/song/distribution/job/projectをscreen名に依存せず表せる。expected failuresがpayload/unionで表せる。
- **Adversarial:** giant `Project.everything`, CRUD mutations, raw filesystem path, arbitrary recursive expensive graph, business errorsをnullable/error propagationで表す案。
- **Verification:** frontend/Rust実装無しでもSDLとsample operationsを独立validatorで検査。
- **Evidence outputs:** schema.graphql、capability map、evolution rules、sample operations。
- **Human review / decision:** public vocabulary、custom scalar necessity、compatibility window。

## T04 — Storage protocol / project writer / migration

- **Objective:** SQLite/filesの成功境界、backup/migration/recoveryを耐障害protocolにする。
- **Prerequisites:** T02,T03。
- **Architecture contract:** sealed final asset後のみtake commit、receipt+revision同transaction、manual orphan recovery、single project writer。
- **Owned / affected:** runtime storage/project、SQL migrations、asset layout、backup/derive。
- **Must not change:** master bytes/history/pins/raw import、unknown enum silent default、orphan auto delete/adopt。
- **Tests:** real SQLite/WAL backup、phase fault injection、kill/reopen、nested asset manifest、legacy corpus、OperationId retry。
- **Observability:** asset phase、commit outcome、migration/recovery events/codes。
- **Migration / recovery:** consistent backup→staging generation→validate→selector switch。ambiguous mappingは停止。
- **Acceptance:** DB row points only finalized asset; response-loss is receipt-recoverable; live WAL copy bug eliminated。
- **Adversarial:** disk full、permission、rename/fsync/DB failure、publish後kill、newer schema、partial assets。
- **Verification:** subprocess killpoints、platform FS qualification、integrity+manifest consistency。
- **Evidence outputs:** migration matrix、fault receipts、backup manifests/hash reports。
- **Human review / decision:** supported filesystems、ambiguous mapping、backup retention、FSL phase refinement。

## T05 — ProjectRuntime / coherent read session

- **Objective:** durable writer/result admissionと、GraphQL Queryが使うcoherent read boundaryを成立させる。
- **Prerequisites:** T02,T03,T04。
- **Architecture contract:** explicit ProjectLease/Epoch、short writer mutations、one-query-one-revision `ProjectReadSession`、lazy facets。ProjectRuntimeをsecond Studioにしない。
- **Owned / affected:** runtime application/project/read projections、revisioning、selection/edit state access。
- **Must not change:** heavy DSP/file copy/native waitsをwriter lockへ戻す、global current projectをconsumer contractとして復活。
- **Tests:** cross-facet query consistency、parallel mutation中read、A→B→A、lease close/open、selection/history integrity。
- **Observability:** project lease lifecycle、read revision、snapshot construction duration、invariant mismatch。
- **Migration / recovery:** existing current-project stateをexplicit leaseへmapping。restart後のepoch reuse禁止。
- **Acceptance:** recording/review/repertoire/editor/distributionが同revisionのfacetとして読める。resolverがDBを個別再読込する必要がない。
- **Adversarial:** query途中commit、old lease、project switch、large facet、lazy facet failure。
- **Verification:** real DB + concurrent scheduler; query responseにsingle revision receipt。
- **Evidence outputs:** read-session contract、latency/memory baseline、consistency tests。
- **Human review / decision:** snapshot granularity、simultaneous multi-project requirementを先回りしないこと。

## T06 — AudioHost / realtime ownership

- **Objective:** capture/playback/control lifetimeをview/projectから分離しRT pathを有界にする。
- **Prerequisites:** T01,T03。
- **Architecture contract:** unique SPSC endpoints、no RT allocation/I/O/log/general lock、explicit Input/Capture/Playback lease。GraphQLはRT callback後段のみ。
- **Owned / affected:** koeru-audio、native backend、pump、observation producer。
- **Must not change:** 44100 master、preroll/tail、channel selection、drop semantics。
- **Tests:** compile-fail !Sync ownership、ring property/Miri/Loom、allocation guard、sample clock、lease state machine。
- **Observability:** input drops/discontinuity/output underrun separate atomics; non-RT aggregation。
- **Migration / recovery:** device setting mapping、disconnect capture outcome。
- **Acceptance:** playback RwLock<Vec> timing coupling除去、bounded buffer、stop independent of DB/job waits。
- **Adversarial:** USB disconnect、saturation、rate change、late old stream observation、callback teardown。
- **Verification:** supported device/loopback + forced unsupported build + RT counters。
- **Evidence outputs:** RT callgraph、unsafe proof、buffer budgets、device matrix。
- **Human review / decision:** unsafe/native API、actual audio quality/guide leakage。

## T07 — JobRuntime / result admission / cancellation

- **Objective:** queue/attempt/cancel/publication/backpressureを一つのprotocolへ集約する。
- **Prerequisites:** T02,T03,T05。
- **Architecture contract:** bounded items+bytes、immutable input stamp、per-attempt cancellation、ProjectRuntime admission、running FFIの強制停止を約束しない。
- **Owned / affected:** jobs、completion routing、derived cache publication。
- **Must not change:** current priority semanticsを根拠なく変更、saved takeをqueue fullで失敗扱い、cancelled result publish。
- **Tests:** deterministic schedule、old attempt、A→B→A、cancel before/after commit、coalesce、saturation。
- **Observability:** job state/attempt/BasedOn/queue wait/inflight bytes/admission reason。
- **Migration / recovery:** resumable chart checkpoint等だけexplicit。old running jobをresume済みと偽らない。
- **Acceptance:** worker closureにDB mutable write authority無し。cancel/AlreadyCommitted/receiptが矛盾しない。
- **Adversarial:** native hang、priority starvation、retry old completion、shutdown/memory pressure。
- **Verification:** fake long capability + real engine representative run。
- **Evidence outputs:** lifecycle traces、state-machine seeds、queue/memory bounds。
- **Human review / decision:** hard isolation/process boundaryが必要なnative failure class。

## T08 — Engines / import / export / material integration

- **Objective:** align/synth/import/exportを新identity/storage/jobsへ接続し、representationとsemantic selectionを分ける。
- **Prerequisites:** T02,T04,T05,T06,T07。
- **Architecture contract:** engines compute only、formats parse/encode、Application decides use/selection/export plan。immutable input/output fingerprints。
- **Owned / affected:** align/synth adapters、material resolution integration、song import、package/export workflows。
- **Must not change:** actual external compatibility、real-audio quality evidence、lossless OTO、down-format behaviorをarchitecture都合で変更。
- **Tests:** engine deterministic inputs、real audio harness receipts、OpenUtau parity、lossless roundtrip、export staging/publish faults。
- **Observability:** engine version/duration、material resolution reason、export phase/receipt、安全なcounts。
- **Migration / recovery:** old derived caches rebuildable、release receipts preserved、raw imports retained。
- **Acceptance:** preview/export resolve same semantic candidates with explicit use-context; no project mutation for export representation。
- **Adversarial:** same alias別tone、stale model results、malformed USTX/ZIP、external changes、publish before receipt failure。
- **Verification:** external-tool compatibility + real DB/assets + real engine selected cases。
- **Evidence outputs:** compatibility matrix、quality receipts、export hashes。
- **Human review / decision:** perceptual/quality thresholds、external conversion policy、licenses。

## T09 — Rust GraphQL adapter / schema parity

- **Objective:** canonical SDLをRust Applicationへ薄く実装し、GraphQL layerを第二のapplication layerにしない。
- **Prerequisites:** T03,T05,T07,T08。
- **Architecture contract:** `koeru-graphql` depends on Application/read session only; domain/runtime/audio do not depend on async-graphql。expected outcomes typed、execution errors exceptional。
- **Owned / affected:** Query/Mutation/Subscription roots/types/scalars、runtime SDL export/parity、resolver context。
- **Must not change:** direct Diesel/native resolver、domain `SimpleObject` derive、SDLをRust生成物の従属物に戻す。
- **Tests:** canonical SDL vs runtime SDL structural parity、resolver integration、same-revision query、mutation idempotency/outcome、subscription source lifecycle。
- **Observability:** graphql operation name/hash/kind、execution/snapshot duration、safe result category。
- **Migration / recovery:** old Tauri command mapping table; no dual semantic implementation。
- **Acceptance:** sample Query/Mutation/Subscription execute in-process without HTTP。all expected domain outcomes are non-`errors[]` payloads。
- **Adversarial:** null propagation as state machine、resolver-local retry、raw source error text、N+1 via per-field DB calls。
- **Verification:** independent SDL parser/validator + async-graphql runtime tests。
- **Evidence outputs:** parity report、resolver dependency graph、operation fixtures。
- **Human review / decision:** scalar choices、schema description discipline、tooling maintenance cost。

## T10 — Generic GraphQL-over-Tauri transport / persisted operations

- **Objective:** desktop transportをapplication semanticsから外し、execute/subscribe/unsubscribeのgeneric bridgeへ集約する。
- **Prerequisites:** T03,T09。
- **Architecture contract:** application-visible stream/command型をTauri Channel/invokeで個別定義しない。Tauri Channelが残るのはGraphQL execution stream transport detailのみ。
- **Owned / affected:** koeru-desktop transport、operation manifest、consumer capability/cost policy、GraphQL client bridge。
- **Must not change:** arbitrary raw GraphQLをproduction GUIへ無制限公開、transportにbusiness decisionsを追加、subscription closeをjob cancelへ変換。
- **Tests:** execute/subscribe/unsubscribe roundtrip、unknown operation hash、capability denial、slow consumer、transport close/reconnect、response ordering。
- **Observability:** transport open/close、operation hash、bytes、subscription queue bytes、coalesce/drop/gap counts。
- **Migration / recovery:** legacy command/channel adaptersはtemporary only; operation mapでequivalenceを追跡。
- **Acceptance:** React featureからTauri invoke/Channel import不要。HTTP/WebSocket server無しでsubscriptionが動く。
- **Adversarial:** channel close mid-event、duplicate subscribe IDs、slow WebView、huge variables、manifest mismatch。
- **Verification:** native Tauri/WebView integration + transport-only unit/integration。
- **Evidence outputs:** persisted operation manifest、transport protocol doc、latency/bytes baseline。
- **Human review / decision:** binary payload optimizationが本当に必要かを実測で判断。

## T11 — Frontend feature colocation / GraphQL + TanStack

- **Objective:** central query factory ownershipをfeature-local fragment + route operation compositionへ移す。
- **Prerequisites:** T03,T05,T09,T10。
- **Architecture contract:** component/feature owns fragment; route/workflow owns operation; generic application/graphql layer owns transport/TanStack integration only。normalized GraphQL cacheは前提にしない。
- **Owned / affected:** UI application client、features、query keys/invalidation、generated TS。
- **Must not change:** feature AがB private hookをimport、componentごとに無計画なIPC request、openProjectをQuery cache lifecycleに隠す。
- **Tests:** all documents validate/codegen drift、restricted imports、route fragment composition、A→B→A、Suspense/waterfall、mutation invalidation。
- **Observability:** operation latency/hash、active query counts、refetch reason。variables全文をlogしない。
- **Migration / recovery:** feature単位でlegacy queriesをreplaceし、同じreadを二契約で長期保持しない。
- **Acceptance:** representative recording/review/distribution routeがcanonical schemaだけに依存しcentral domain query factory不要。
- **Adversarial:** duplicate fragment name、overfetch huge list、route unmount during mutation、stale response、generated doc collection 0。
- **Verification:** browser + real GraphQL runtime + native Tauri selected journey。
- **Evidence outputs:** operation/fragment dependency graph、bundle/codegen reports。
- **Human review / decision:** component boundary/UX data dependencyが過細分割になっていないか。

## T12 — Subscription unification / stream semantics

- **Objective:** project/job/envelope/playback等のapplication-visible streamをGraphQL Subscriptionへ全面統一する。
- **Prerequisites:** T06,T07,T09,T10,T11。
- **Architecture contract:** each subscription has lifetime owner、sequence/cursor、replay/resync、backpressure/drop policy。unsubscribe≠application cancel。
- **Owned / affected:** subscription sources、frontend transient stores、legacy Channel removal。
- **Must not change:** RT callback内GraphQL/serialization/logging、durable eventをmeter同様にdrop、high-frequency framesをTanStack cacheへ毎回書く。
- **Tests:** project cursor gap/resync、job current-state+events、envelope slow consumer/latest-wins、playback closure、unsubscribe without cancel、explicit cancel mutation。
- **Observability:** sent/coalesced/dropped、queue bytes、cursor gap、subscribe lifecycle、RT-to-subscription latency。
- **Migration / recovery:** legacy stream adapterを一つずつ除去。durable stateはQueryで再同期可能にする。
- **Acceptance:** production source treeにapplication-specific Tauri `Channel<T>` contractが無い。all stream consumers use GraphQL documents。
- **Adversarial:** event gap、out-of-order reconnect、rapid mount/unmount、consumer stall、project switch、job completes after unsubscribe。
- **Verification:** deterministic stream model + WebView integration + real meter/job selected tests。
- **Evidence outputs:** stream policy table、queue budgets、channel-removal report。
- **Human review / decision:** transient frame rate/coalescing UX、binary payload threshold。

## T13 — M6 representative editor vertical slice

- **Objective:** GraphQL Query/Mutation + fragment colocation + coherent read + WASM kernelでM6の難所を通す。
- **Prerequisites:** T02,T04,T05,T11,T12。
- **Architecture contract:** same Rust editing kernel native/WASM; per-frame local proposal、durable commit only Mutation。EditingSession mode is application state; view layout mode is frontend local。
- **Owned / affected:** editor read facets/windowing、ApplyEditorCommand、bulk preview/commit representative path、WASM facade、React editor fragments。
- **Must not change:** 8ms/undo/manual protection/invalid numeric/lossless semanticsをGraphQL往復で悪化、arbitrary scripting runtime追加。
- **Tests:** native/WASM parity、pointer/keyboard/IME、stale reestimate vs manual edit、one-undo、5000 entry window/bulk performance、fragment data dependency。
- **Observability:** edit operation/receipt、conflict/protected counts、gesture/commit/window query latency。pointermove trace禁止。
- **Migration / recovery:** existing editor/review data identity mapping、history/provenance preservation。
- **Acceptance:** representative normal/advanced workflowが同truthを扱い、route/view changeでmode/session invariantsが壊れない。
- **Adversarial:** advanced session closes before delayed mutation、bulk preview stale、5000 rows overfetch、invalid draft、late job result。
- **Verification:** browser + WASM + native commit + real SQLite + budget harness。
- **Evidence outputs:** parity fixtures、performance receipt、editor operation documents。
- **Human review / decision:** IME/focus/pitchmark/normal-mode correction quality。

## T14 — Errors / diagnostics / consent / GraphQL privacy

- **Objective:** caller action、GraphQL expected outcome、trace/metrics/privacy、consentを一つのdiagnostic contractへ揃える。
- **Prerequisites:** T03,T05,T07,T09,T12。
- **Architecture contract:** domain rejection/conflict/AlreadyCommittedはpayload/union。`errors[]`はunexpected execution failure。GraphQL text/variablesはnormal logs/telemetry禁止。
- **Owned / affected:** error taxonomy/catalog、diagnostic constructors、support bundle、ConsentStore、telemetry gate、GraphQL extensions/tracing。
- **Must not change:** telemetry default off、crash/telemetry consent分離、raw path/audio/lyrics/alias/project namesのnormal export。
- **Tests:** error mapping、source-error redaction、GraphQL variable canary、consent enqueue/revoke/send race、bundle content、subscription diagnostics。
- **Observability:** operation/schema/manifest hash、safe codes/counts/durations、subscription stats。high-cardinality IDsをmetrics labelにしない。
- **Migration / recovery:** old AppError mapping to stable outcomes; support bundle old fields remove safely。
- **Acceptance:** failure debugging can follow GraphQL operation→OperationId→commit/job/subscription without sensitive content。
- **Adversarial:** `#[instrument(err)]` path leak、resolver debug error、free-string GraphQL extension、telemetry retry after revoke。
- **Verification:** outlet canaries + failure worked examples + no-network consent tests。
- **Evidence outputs:** error/outcome catalog、privacy schema、support-bundle sample。
- **Human review / decision:** SaaS provider/retention/location remains blocked until decided。

## T15 — Integrated fitness / portfolio / performance / schema gates

- **Objective:** architecture rulesをcompiler/linter/schema validators/test receiptsで機械化し、false greenを防ぐ。
- **Prerequisites:** T04–T14。
- **Architecture contract:** enforceable rulesは機械化、quality/native/human evidenceはobserved gatesとして区別。
- **Owned / affected:** CI/xtask、portfolio manifest、schema/document checks、dependency/import checks、performance runners。
- **Must not change:** required suite missing fixtureをpass化、shared runner noiseを絶対performance gate、custom validatorsを重複乱造。
- **Tests:** negative canaries for SDL/runtime drift、0 GraphQL docs、missing operation manifest、feature Channel import、resolver direct storage dep、privacy variables leak、0 native calls。
- **Observability:** machine receipts include git/schema/runtime-schema/operation-manifest hashes、discovered/executed operations/subscriptions/tests。
- **Migration / recovery:** fixture/version corpus itself versioned; no user data destructive checks。
- **Acceptance:** cargo/dependency graph、SDL parity、document validation/codegen、persisted operations、subscription policy、test portfolio all green with evidence receipts。
- **Adversarial:** empty globs、stale generated files、unsupported backend、fake subscription no events、deprecated field still used、budget bypass。
- **Verification:** clean CI replica + isolated canary mutations + platform/nightly/release split。
- **Evidence outputs:** fitness report、negative-canary receipts、performance baseline。
- **Human review / decision:** what becomes hard release gate vs advisory until runners stabilize。

## T16 — Coherent cutover / legacy application contract removal

- **Objective:** GraphQL/application runtimeへproduction pathを一度に切替え、旧Specta/Tauri application contractを除去する。
- **Prerequisites:** T15。
- **Architecture contract:** one production application contract。Specta/Tauri application commands/channelsをcompat facadeとして残さない。host-shell APIsは例外。
- **Owned / affected:** composition root、legacy Studio/commands/queries、generated bindings、migration rollout。
- **Must not change:** old write path permanent dual-run、unsafe data migration、rollback名目でtwo writersを同時有効化。
- **Tests:** full representative journeys、migration/reopen、native device、export、all persisted operations、no legacy registration/imports。
- **Observability:** startup contract/schema hash、legacy path invocation counter must be zero/removed、cutover failure diagnostics。
- **Migration / recovery:** pre-cutover coherent backup、new generation validation、safe fallback before first new write as defined; after cutover one writer only。
- **Acceptance:** production bundle has canonical GraphQL consumer surface + generic Tauri transport only; old application-specific bindings absent。
- **Adversarial:** stale frontend bundle、schema hash mismatch、rollback after new write、forgotten test-hook command/channel。
- **Verification:** clean install/upgrade/reopen across target platforms; dependency/import scan。
- **Evidence outputs:** legacy removal manifest、upgrade receipt、bundle inspection。
- **Human review / decision:** release migration/rollback window and user communication。

## T17 — Contributor governance / Agent Skills

- **Objective:** post-refactor architectureをcontributor/agentが再現可能に守れる形へcanonicalizeする。
- **Prerequisites:** T16。
- **Architecture contract:** Skills are workflow/control plane, not canonical semantics. Contributor guide points to FSL/meta/SDL/owner/test/error/operation manifests。
- **Owned / affected:** AGENTS/contributor guide/architecture index/owner map、`plan-koeru-change`、updated `verify-koeru`、skill evals/package。
- **Must not change:** canonical rulesをSkillへコピー、old crate/path namesをhardcode、skills implementationでproduction architectureを再設計。
- **Tests:** trigger positive/negative, GraphQL contract changes, subscription changes, pure internal change, missing native fixture, stale SDL/runtime drift。
- **Observability:** skill verification receipt references tested SHA/schema/manifest hashes; no telemetry requirement。
- **Migration / recovery:** old skill references/path updates; symlink/package conventions preserved。
- **Acceptance:** new contributor can classify Query/Mutation/Subscription/host/internal, place code, choose tests, detect schema/channel drift without architecture author。
- **Adversarial:** cosmetic UI change overtested、Rust struct change unnecessarily modifies SDL、new Channel added for convenience、MCP bypasses GraphQL executor。
- **Verification:** representative real changes + negative cases + skill validator/package/install smoke。
- **Evidence outputs:** canonical reference map、skill eval report、individual `skill.zip` artifacts。
- **Human review / decision:** skill trigger scope, maintainer ownership, documentation wording。

## H6 — M6 全要求の実装・品質判定

- **Objective:** editor/review/audition requirements全体を新architecture上で完成させる。
- **Prerequisites:** T16。T13は代表sliceであり全M6要件ではない。
- **Architecture contract:** one document truth、GraphQL fragments/projections、same editing kernel、protected edits、bounded bulk/windowing、lossless import。
- **Owned / affected:** remaining M6 feature implementation。
- **Must not change:** architecture refactor完了をM6 completionと偽らない。
- **Tests:** requirement-to-test map、5000 entries、30min chart、external roundtrip、real audition、a11y/IME。
- **Observability:** product quality + performance budgets, not arbitrary usage telemetry。
- **Migration / recovery:** existing edits/history/import raw data preserved。
- **Acceptance:** all M6 TR/FSL acceptance + evidence/human gates。
- **Adversarial:** large projects、invalid imported data、manual vs auto conflict、subscription stalls。
- **Verification:** release-like builds and human/native checks。
- **Evidence outputs:** M6 qualification report。
- **Human review / decision:** unresolved assumptions/Qs closed or explicitly blocked。

## H7 — M7 実環境 / 配布 / privacy qualification

- **Objective:** 3OS distribution/signing/update/accessibility/telemetryをproduction qualifyする。
- **Prerequisites:** T16。
- **Architecture contract:** same canonical GraphQL schema/capabilities across platforms; unavailable backend is not functional support。
- **Owned / affected:** platform backends、packaging/update、a11y、consent/telemetry adapter。
- **Must not change:** default-off consent、privacy fields、support claims without native evidence。
- **Tests:** 3OS native positive tests、installer/update/rollback、screenreader/IME、consent/revoke/privacy、schema hash compatibility。
- **Observability:** release/build/schema/operation-manifest versions、safe backend/result metrics。
- **Migration / recovery:** upgrade/rollback data compatibility、queued telemetry revoke semantics。
- **Acceptance:** M7 profile evidence complete; Q-TEL provider decision resolved before actual sending。
- **Adversarial:** signed stale frontend/schema mismatch、update during migration、unsupported audio on claimed OS、telemetry retry after revoke。
- **Verification:** distribution artifacts/hashes and human platform qualification。
- **Evidence outputs:** M7 release qualification report。
- **Human review / decision:** signing/provider/legal/privacy/retention/location/licensing。
