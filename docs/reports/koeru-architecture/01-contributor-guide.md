# Contributor Architecture Guide — 変更の判断用 handbook

本書は採用前の handbook 案である。refactoring 完了後は `docs/contributing/architecture-guide.md` 相当へ置き、確定した ownership map と accepted DEC を参照させる。ここに出る新しい file path は提案であって、現 repository に存在するとは限らない。FSL/meta の仕様をここへ複製しない。

## 0. 変更を始めるときに記入する9行

```text
User intention / affected contract IDs:
Truth changed / affected object identities:
Decision owner / commit owner / resource owner:
Consumer contract classification: Query / Mutation / Subscription / host-only / internal-only:
Canonical SDL / operation / fragment impact:
Lifetime / execution class:
Stale, duplicate, partial-success, unsubscribe/cancel scenarios:
Cheapest test that would detect a wrong implementation:
Caller action + diagnostic signal + required human evidence:
```

最初に `AGENTS.md`、architecture index、canonical `schema.graphql`、`cargo xtask touched`、関連TR/FSL/DEC/Qを読む。仕様が無いところを directory naming から埋めない。新しい番号は `cargo xtask next-id`。Skillの文章を仕様として引用しない。

## 1. Placement decision tree

**その変更が何を決定するか**から順に答える。

| 問い | 置き場所 | 置かない場所 |
|---|---|---|
| 同じinputから同じproduct判断を作るか | model内の既存 owner。例: candidate解決、編集制約、保護、eligibility | React、Tauri command、SQL queryの副作用 |
| 何をいつ永続化し受理するか | runtimeの能力module＋project writer/storage protocol | workerのcompletion closure、UIのonSuccessだけ |
| 外部表現のsyntax/encoding/lossless roundtripか | formats | selection/review model、native audio |
| OS device/native handle/callbackの責任か | audio ownerまたはengine adapter | ProjectRuntimeにポインタを共有 |
| 任意に遅れる重い計算か | job input/output contract＋実際のengine | writer lock内、React effect内 |
| readのための集計・整形か | runtimeのcoherent read facet + GraphQL Query field。表示上のspacing/labelはReact | resolverごとのDB再読込、DB rowそのままpublic化 |
| focus/drag/zoom/選択/モードだけか | feature/view-local state | durable project設定、global query cache |
| 音声の進捗/levelか | bounded observation owner→GraphQL Subscription→feature-local RT store/rAF | application-specific Tauri Channel、every-frame query invalidation |
| plugin/MCPで必要そう、しか根拠がないか | 今は作らない。仮説をdecisionのreview triggerへ | generic registry/DI container/event bus |

### Existing owner に置く条件

不変条件・変更理由・lifetime・execution class が一致するなら、多少fileが長くてもまず既存ownerのprivate moduleへ置く。既存ownerがUIとDBとnativeを同時に持つ場合は「既存だから」で押し込まず、責任混在として分割する。

### New module にする条件

既存機能と独立に変更・検証できるpolicy/algorithm/protocolを持ち、private interfaceを説明できるとき。単にAPI endpointを一つ増やす、200行を超えた、feature名が増えた、だけでは作らない。

### New crate にする条件

moduleでは機械的に守れないnative SDK/FFI、build target、consumer API、依存許可、compile isolationが実際に必要なとき。crate増でbuild/feature matrix/公開API/maintainer責任が増える費用を記録する。独立releaseが無ければworkspace内部crateでよい。one-entity/one-use-case/one-feature/one-layer の慣習では切らない。

### Frontend-local にする条件

画面を閉じてもproduct truthが失われない状態、または未確定gesture/draftを表示する責任。未送信draftを保持するUX契約がある場合はその契約を明示し、localという理由で消してよいとしない。5値のbusiness判断は同じmodel kernelを使う。

### Shared に昇格させる条件

複数consumerが**同じ意味・同じ変更理由**を必要とし、ownerとpublic contractを説明できるとき。似たJSX、同じfield名、2回出現した、は不十分。generic UIはdomainを知らない。cross-feature business連携はapplication capabilityまたはroute compositionに上げる。

### Application/public contract に出す条件

consumerが独立に要求する意図、読むべきprojection、観測するlong-lived state、判断するfailureであるとき。正本は canonical GraphQL SDL。内部都合の「DB行を更新」「cacheを消す」「Studioのこの関数を呼ぶ」は外へ出さない。Rust structをGraphQL objectへそのままderiveしてcontract ownershipを戻さない。rawpath/nativehandle/SQLtypeをpublicにしない。

分類は次の順序で決める。

- **Query:** stateを変えず coherent snapshot/projection を読む。高コストresourceを予約する処理はQueryに偽装しない。
- **Mutation:** user intention、resource/job creation、durable write、application cancellation。
- **Subscription:** evolving observation。durable change notification、job lifecycle、transient meter等。unsubscribeは通常「観測を止める」であってoperation cancelではない。
- **Host-only:** file picker/window/reveal等、KOERU application languageではなくdesktop shellの能力。
- **Internal-only:** consumerが直接要求する理由がない実装detail。

## 2. Dependency の選択

| 選択 | 適切な場合 | 危険な兆候 |
|---|---|---|
| direct call | 同一owner内、pure computation、小さい明示依存 | feature AがBの内部状態を読む |
| public interface | 別ownerの安定した能力を使う | interfaceがprivatefieldsの写し |
| adapter/port | OS/engine/transport等の実際の外界境界 | test用だけのRepository<T>やService<T>が量産 |
| message | 実際にthread/lifetimeが異なるcontrol、job input/completion | 同期pure関数までactor化、無制限queue |
| event | committed changeの通知、進捗・診断 | eventを受けた順でdurable invariantを成立させる |
| shared read model | 複数viewが同じcoherent snapshotを必要 | clientがread modelをそのままwrite-back |
| abstractionなし | 一つの具体SQLite実装、pure function、単純value | 将来差替え可能性だけでtraitを作る |

**依存方向:** consumer→canonical GraphQL operation→`koeru-graphql`→Application/runtime。runtime→model/formats/native adapters。model/runtime/audioはGraphQL/Tauriを知らない。resolverはstorage/native engineを直接呼ばない。engineはuser selectionを決めない。formatsはadoptionを決めない。React feature同士はprivate hookをimportせず、fragmentでcanonical schemaへ依存する。runtime内storageは具体implementationをprivateに持ち、全CRUDをgeneric repositoryにしない。

## 3. State / lifetime / async review

永続entityのID、操作のOperationId、project epoch、job attempt、view generationを混ぜない。音高/aliasが変わっても何が同じ対象か説明できなければschema変更を始めない。

長い処理を始める前にinput snapshotとrevisionを確定する。GraphQL Queryでproject由来の複数fieldを返す場合は一つのProjectReadSession/revisionに固定する。workerにはDB mutable referenceを渡さない。結果の受理はownerに戻して行い、project/target/revision/attempt/cancel/protected-fieldsを検査する。cleanupで共有の録音/再生を止めるときは自分のleaseだけを解放する。

操作ごとに次の答えをPRに書く: A→B→Aで遅れて届いたらどうなるか、retryが二重writeになるか、cancelとcommitが競合したらどちらが勝つか、fileだけ保存されたらどう見えるか、queueが満杯なら録音済みtakeはどうなるか。Subscriptionではさらに、unsubscribeとapplication cancelの違い、sequence gap、replay/resync、backpressure、drop/coalesce policyを書く。

## 4. Risk から test を選ぶ

| 変更risk | 最初に書くtest | 追加確認 |
|---|---|---|
| identity/alias/tone/selection | pure invariant/property＋実DB constraints | 同じtargetの別row、same alias別tone、historical/adopted |
| 編集規則/undo/pin | kernel command roundtrip/state machine | native/WASM parity＋browser gesture |
| syntax/encoding/import | raw byte fixtures、roundtrip、fuzz/property | 実外部tool互換、壊れたfileをsilent skipしない |
| async/stale/retry | deterministic schedulerでcompletion順序を入替 | application integrationで同じreceipt/error/eventを確認 |
| DB/FS/migration | temp real SQLite＋fault injection＋reopen | childprocess killpoint、旧project corpus、platformmatrix |
| nativeaudio/FFI | adapter contract、RT allocation/SPSC checks | real backend、loopback/device disconnect、実音声 |
| UI interaction | component/browser実イベント、CSSを適用したaxe | native WebViewのIME/focus/menu/permission、人のscreenreader |
| GraphQL schema/operation | canonical SDL parse/validate、runtime SDL parity、documents/codegen drift | Tauri execute/subscribe roundtrip、deprecation/compatibility |
| performance | 該当user-visiblebudgetを代表scaleで測る | isolatedrunner/実機、microbenchは原因追跡用 |
| privacy/diagnostic | canary sensitivevalueを全出口に流す | bundle検査、withdrawal中の送信race |

「Rustファイルだからunit」「Reactだからstory」「重要だから全部E2E」では決めない。test数ではなく、意図したbugを入れたら最も安いseamが赤くなるかを見る。実行されていないtestをpassと報告しない。missing model/env/fixtureはrequired suiteではfail、任意harnessでは明示not-runにする。

## 5. Error の置き方

新failureを追加するとき、callerが次にできることを先に決める。`InvalidInput`は構文/型/サイズ違反、domain rejectionは有効requestだが状態上不可、conflict/staleはread refresh/再preview、cancelは正常terminal、Busyは待つ/明示retry、storageはdurable outcomeを照合、corruptは隔離/recovery、unsupportedは別capability、bugは停止・diagnosticとする。

errorをcrate名だけで区切らない。低層のsource chainは内部へ保持し、public boundaryではstablecode・safe structuredcontext・許されるaction・operationreceiptを返す。Expected application outcomeはGraphQL payload/unionで表し、通常のdomain rejection/conflict/AlreadyCommittedをGraphQL `errors[]` に逃がさない。`errors[]` は invalid document/resolver bug/unexpected invariant等に限定する。UIでDisplay文字列をparseしてretry判定しない。GraphQL null propagationをapplication state machine代わりに使わない。`message`をそのままtrace/telemetryへ渡さない。invalid editor draftは受理可能なdocument＋issuesであって、常にAPI errorとは限らない。

## 6. Tracing / event / metric / quality の選択

| 判断したいこと | 出すもの |
|---|---|
| どの意図/GraphQL operationからどのjobへ進んだか | operation name/hash/kind + OperationId span＋correlation/link |
| commit/拒否/復旧/取消のどこへ到達したか | bounded structured lifecycle event |
| どの工程で時間を使ったか | durationsとqueue-wait、trace samples |
| xrun/overflowが何回増えたか | callback atomic counter→非RT集計 |
| 常駐bytes/queue/framesが予算内か | bounded-label gauges/histograms |
| 音素境界の品質、手修正率はどうか | versioned evaluation/quality datasetと評価結果 |
| 利用者環境から送るか | 別consent/allowlist/schemaを持つtelemetry adapter |

GraphQL document本文やvariables全文をtraceしない。operation name/hash/kind、safe counts/bytes、schema hashを使う。function entry/exit全件をtraceしない。OperationId/ProjectEpoch/TakeId/JobIdはlocal相関に使えるがmetric labelにはしない。rawpath、lyrics、audio、projectname、alias本文、secretをログへ出さない。opaque IDも外部へ送るならlinkability審査が必要。callbackは通常tracingを呼ばず、事前確保counter/buffer以外に書かない。

## 7. Mechanical enforcement の置き場

採用時に次をcanonical indexへ登録する。新configをmeta配下へ勝手に置かず、必要なschema/validatorを同時に追加する。

| 正本 | 役割 | 強制 |
|---|---|---|
| FSL/meta、既存xtask | product contract/DEC/Q/profile/budget | lint/chain/refinement/references/coverage/index |
| Rust visibility、Cargo manifests | private/public境界、nativefree target | compiler、cargo metadata dependency allowlist |
| canonical `schema.graphql` | consumer-facing application vocabulary/capabilities | SDL parse/validate、runtime-exported SDL structural parity、breaking/deprecation check |
| GraphQL operations/fragments + generated artifacts | feature data dependencies、production operations | all documents validate、codegen diff、operation/fragment discovery receipt |
| persisted-operation manifest | operation hash/kind/capability/cost/schema hash | production build completeness、unknown operation rejection |
| ownership/config map（例 `.architecture/owners.toml`） | module paths、owner、forbidden edges | cargo metadata/既存xtaskで GraphQL/Tauri forbidden edges も検査 |
| frontend import config | feature間private import禁止、featureからTauri invoke/Channel禁止 | 既存linterのrestricted-imports等。独自JS parserは作らない |
| portfolio manifest（例 `tests/portfolio.toml`） | required suite/backend/fixture/expected discovery | list→run→receipt。0件とearlyreturnを見分ける |
| diagnostic schema/typed constructors | 許可fields、error/event形 | compiler＋outlet canaries＋既存allowlist scan |

test-only concernでproduction APIを汚さない。fault injectionはstorage protocolの実際のphaseで制御し、global ENVを各production関数へ埋めない。低コストcompiler/linterを先に使い、custom toolは既存xtaskに局所機能として加える。guardを追加したらnegative canaryも用意し、例外には理由・owner・再評価条件を付ける。

## 8. PR 完了カード

```text
Contract IDs and any product decision:
Owner / dependencies / lifetime / execution:
Public contract / canonical SDL / operation-fragment changes:
Durable result and recovery/migration:
Tests: planned / discovered / executed / passed / failed / not-run:
Adversarial scenarios actually exercised:
Errors / GraphQL outcome-vs-execution-error / subscription semantics / trace events / measured budgets:
Privacy and license review:
Generated SDL parity / operation manifest / codegen / architecture drift checks:
Human/native/perceptual verification outstanding:
Evidence paths + tested SHA + backend/fixtures:
```

すべてを毎回full E2Eで埋める必要はない。非該当は理由を記す。新architectureのownershipを守る変更かどうかと、productの正しさを検証したかどうかを別にレビューする。

## 9. GraphQL contributor rules

1. application-visible contract changeはcanonical SDLを先にreviewする。Rust typeから生成されたshapeを正本にしない。
2. GraphQL objectはdomain/DB rowをそのままexportしない。resolverはApplication/ProjectReadSessionのみを呼ぶ。
3. feature/componentのdata dependencyは近傍fragmentで宣言し、route/workflowがoperationをcomposeする。componentごとに独立requestを増やす必要はない。
4. application-visible streamingはSubscription。featureからTauri `Channel` / raw invokeを使わない。generic GraphQL transport実装だけがTauri Channelを知ってよい。
5. durable event、job progress、transient meterで同じdelivery policyを使わない。sequence/replay/resync/backpressure/drop policyを定義する。
6. production operationはmanifestへ登録し、capability/cost classを付ける。GraphQLの自由度をunbounded resource accessとして公開しない。
7. deprecated field/argument/inputはoperation usageが0になるまで削除しない。internal refactorだけでschemaを変えない。
8. high-frequency Subscription payloadをTanStack Query cacheへ毎frame入れない。feature-local external storeを使う。
9. GraphQL text/variablesを通常logging/telemetryへ出さない。

## 10. FSLに意味があるmodeの例外

frontend-localなmodeとはレイアウトや表示方式を指す。normal/advanced/unconstrainedのようにFSLが許可operationを定めるmodeは、consumer/project leaseのephemeral EditingSessionに置き、native/kernelでも検査する。mode切替でprojectのentry/historyを変えないことと、backendがmodeを一切知らなくてよいことは同じではない。closed sessionから遅れて届くadvanced editを受理しない。この区別はnewconsumerを足す際にも維持する。