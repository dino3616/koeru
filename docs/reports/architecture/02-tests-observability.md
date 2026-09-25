# KOERU — Risk-based Test Strategy / Error / Observability

設計提案。既存の事実は `06-evidence.md` のR番号、新しい検査・codes・eventsは提案である。本資料のtestsは今回実行していない。

## 1. 既存 suite を何の証拠として使えるか

| 観測 | 証明できること | 証明できないこと |
|---|---|---|
| FSL chain/refinement、meta/reference/generation checks | 登録されたモデル・参照・生成物の整合 | Rust/FS/Reactがモデルを実現したこと、モデルがproduct意図に一致すること |
| pure/structural DSP tests | 単調性・決定性・形状・一部の数学性質 | 実音声の境界位置、聴感、native ABI |
| real-audio harness | env/modelが揃い実行された場合のpower-span sanity | ラベル付き20ms精度、全音素・全話者、未実行環境の成功 |
| Storybook + play + axe | CSS適用下の収集済みcomponent interaction/一部a11y | 全route、IME、OS permission、実Tauri transport、実録音 |
| forced unsupported build | 未対応OS用分岐もbuildしUnavailableを扱える | 対象OSにnative backendが存在し動くこと |
| OpenUtau fixed stable/alpha parity | 固定hashのphonemizer期待値との一致 | arbitrary外部version、UTAU本体の音質、全配布packageの動作 |
| 最新CI Ubuntu log | 実際に走ったbinary/ケースとtrace guard failure | 途中停止後の全workspace合格、人のDSP review |

PR3に0件green、未適用CSS、tsc parentのみ、awaitされないassert、trace scanner漏れが記録されている。CMVNは構造testを全部通ったまま実音声を壊した。これらは「unitが無価値」「E2Eを増やせばよい」ではなく、oracleと実行経路を選ぶ必要があるという証拠である。[R19,R21–R24]

## 2. Risk → cheapest reliable test → higher confirmation → production signal

実行記号: C=every commit/PR、P=platform matrix、N=scheduled/nightly、R=release gate、H=human verification。Cで安い決定論的ケース、Nで広いseed/重い障害注入、Rでrelease buildと対象環境を確認する。

| Risk / Contract | Cheapest reliable test | Higher-level confirmation | Production signal | 実行 |
|---|---|---|---|---|
| same semantic targetが違うrow/表記で重複、same aliasが別tone | material resolver invariant/property、順序変更metamorphic | realSQLite候補/selection制約、PR16 repacked-row use case | resolution conflict / missing-material reason | C,N |
| historical takeの表示に対しadopted takeを更新 | application mutationにtarget/candidate/revisionを要求するtest | browserで旧takeを開きGraphQL Mutation/receipt確認 | rejected target/selection conflict | C,P |
| project A→B→A、遅延read/write | deterministic state-machine、epoch再利用なし | actual GraphQL client+runtimeでoperation/subscription順序入替 | stale_epoch rejection、lease lifecycle、cursor gap/resync | C,N |
| retry / duplicate / lost response | 同operation＋同/異payloadのreceipt invariant | DB commit後にresponseを落としreopen/retry | duplicate accepted / conflicting key / outcome lookup | C,N,R |
| user edit中に古い解析完了 | model protected-field test＋revision/attempt admission | realDB + controlled worker completion | completion.discarded{reason}、protected count | C,N |
| cancel後のcache増殖、commit/cancel競合 | publicationの前後を固定したscheduler test | actualrender workerのcancel harness | cancelled_before_publish / already_committed / cache bytes | C,N,P |
| native jobが止まらない、queue飽和 | bounded admission property、長いrunning job stub | realengine遅延＋capture/stop responsiveness | queue wait、inflight bytes、cancel latency | C,N,P |
| SPSCを安全に複数producer化できる | compile-fail Send/!Sync、排他的push/pop API | Miri/Loomによる限定model、native stress | ring overflow/drop counters | C,N,P |
| callback allocation/lock/destruction | RT callgraph review＋test allocator guard＋boundedbuffer checks | loopback負荷試験、callback timing/xrun | input discontinuity/output underrun | C,N,R,H |
| 48k→44.1k timebase誤り、preroll/tail欠落 | rate/frame-count property、impulse/tone metamorphic | actual device/header+duration、音頭聴取 | format mismatch / discontinuity / effective rate | C,P,R,H |
| WAV write/flush/rename/DB failure | real tempfilesystem+SQLite phase fault injection | childprocess killpoints、再起動/recovery、supported FS | capture phase / durable outcome / recovery candidate | C,N,P,R |
| snapshotがWAL commitやnested assetsを失う | uncheckpointed WALを作りbackup/reopen、manifest全資産照合 | live project snapshot/derive・interrupt | backup verified revision / missing asset | C,N,P,R |
| 旧project migrationのデータ損失 | versioned legacy fixture→migrate→hash/count/pins/rawtext比較 | interrupted migration selector切替、実ユーザー由来の許諾fixture | migration result/source/target version、recovery state | C,N,P,R,H |
| malformed UST/USTX/OTO/ZIP、巨大入力 | codec fuzz/property、size/depth limits、path traversal fixture | import use caseでpartial/diagnostics/取消 | import rejection code、bounded bytes/count | C,N |
| OTO無編集でbytes変化、blank=0化 | exact byte roundtrip corpus（encoding/unknown lines） | external tool/CP932/UTF-8/複数音階往復 | roundtrip/encoding issue、external-change classification | C,N,R,H |
| normal/advancedで別truth、invalid draft消失 | same document command model、numeric/importとdrag別oracle | browser table/drag/mode/undo interaction | document violations count、command receipt | C,N,H |
| native/WASM編集規則のdrift | 同一fixture/seedのnative/WASM outputs比較 | actual pointer+modifier+snap+keyboard＋native commit | invalid proposal / stale edit / gesture duration | C,P,N |
| bulk13操作でundo不完全・無制限式 | command/inverse property、bounded expression interpreter tests | 5000件実DB、20件preview、one-step undo | affected/protected/rejected count、bulk duration | C,N,R |
| canonical SDL / runtime schema / operation drift | SDL parse+validate、runtime SDL structural parity、all documents validation、codegen diff | realTauri GraphQL execute/subscribe roundtrip、deprecated/breaking compatibility fixtures | schema/operation hash mismatch、decode/validation error | C,P,R |
| GraphQL query内でrecording/reviewが別revisionを読む | ProjectReadSessionが1revisionを固定するintegration test | parallel mutation中にcross-facet queryを反復 | query revision mismatch counter / invariant failure | C,N |
| Subscription unsubscribeがjob cancelになる | observation lifecycle state-machine | React unmount/reconnect中もjob継続、明示cancelだけ停止 | subscribe close reason / job cancel reason | C,N,P |
| durable event欠落 / transient stream飽和 | sequence gap・delivery-policy model | generic Tauri subscription bridgeでgap/coalesce/backpressure | cursor gap、events sent/coalesced/dropped、queue bytes | C,N,P,R |
| application-specific Channelが復活 | dependency/import/static check | production bundle/registration inspection | architecture drift gate | C,R |
| persisted operation/capability/cost manifest drift | operation discoveryとmanifest completeness | unknown hash / forbidden capability / over-budget operation rejection | operation reject reason、manifest/schema hash | C,N,R |
| fragment colocationがfeature couplingへ退行 | restricted imports＋document graph inspection | route operationがchild fragmentsをcomposeするbrowser integration | operation/fragment usage report | C,N |
| tests collection=0 / earlyreturn | required portfolio listとexecuted receiptの照合 | 意図的config破壊/fixture除去でgateが赤か | build evidenceのnot-run/unsupported表示 | C,N,R |
| unsupported backendがfunctional対応に見える | capabilityUnavailable response、cfg別discovery matrix | 対応を名乗るOSでnative機能positive test必須 | capability/engine availability、unsupported code | C,P,R |
| UI focus/IME/keyboard/a11y不良 | component/browser events、CSS付きaxe、await asserts | nativeWebView IME、screenreader、permission/menu | optional UX error、focus bugのsanitized repro | C,P,R,H |
| previewとexportが別素材/別設定を使う | same resolved material planを異なる用途で検証 | OpenUtau parity、actualpackage読込＋preview聴取 | plan fingerprint、resolution reason、export receipt | C,N,R,H |
| alignment position誤り・confidence誤解 | licensed real-audio sanity＋ラベル付きquality set | 話者/音素/収録条件別、盲検修正・聴感 | local quality observations、evaluation revision | N,P,R,H |
| performance/memory regress | representative workload macrobench＋byte-budget assertions | isolatedrunner、process tree RSS、first audible/frames | budget margin、queue wait、frame misses、xrun | C(軽量),N,P,R |
| sensitive trace/bundle/telemetry漏洩 | typed fieldsとcanary値で全出口検査 | bundle preview、opt-in/off/revoke競合network sink | privacy gate rejection、consent state | C,N,R,H |
| file watcherが自分のexportを再import | content fingerprint＋operation receipt state-machine | OS recursive watcherでown/external/mixed edits | external_change_detected/own_write_suppressed | C,P,N |
| device disconnect / shutdown | AudioHost lifecycle model＋ownerlease tests | USB切断、defaultdevice変更、終了中capture/FFI | disconnect、stop failure、pending recovery | C,P,R,H |

## 3. 日常・platform・nightly・release・human の分担

### Every commit / PR

pure invariant/propertyの固定seed、application state-machine、realSQLite/temp FS、lossless formats、migration最小corpus、native/WASM parity、canonical SDL/runtime parity、GraphQL documents/codegen/operation manifest、frontend component/interaction、contract/FSL/meta、型/lint/license/forbidden deps。riskが変われば該当casesを追加する。録音やFFIに触るPRはnative matrixも必須に上げる。変更pathsだけのtest選択にせず、contract IDsも使う。

### Platform matrix

macOS/Windows/Linuxのcompile、supported/forced-unsupported両分岐、rustdoc cfg、filesystem rename/flush/reopen、native engine load、generic GraphQL-over-Tauri execute/subscribe/unsubscribe transport、WebView basic interaction。未実装backendは「build/Unavailableのみ」を正しい結果とし、functional support欄を未達のまま残す。M7を名乗るreleaseは3OS native positive evidenceが必要。

### Scheduled / nightly

広いproperty/fuzz seed、Loom/MiriのRT core、process killpoints全phase、migration全version、大きいproject、長時間queue/cache soak、real-audio評価、性能runner、guard canary。nightly failureを無期限に放置してreleaseだけgreenにしない。flaky testはquarantine理由とownerを持ち、該当contractは未保証と表示する。

### Release gate

配布予定のbinary/hashでnative smoke、署名/update/rollback compatibility、migration/recovery、release flagsにtest driver/debug endpointが無いこと、privacy/consent、代表scale budget、固定外部engine compatibility、許諾済み実音声、代表device。CIがあるというだけでhuman gateをcheckedにしない。

### Human verification

実microphone/noise/clip/guide leakage、speaker/headphone、device disconnect、長時間収録、実音声の境界修正と聴感、IME/keyboard/screenreader、migrationの曖昧なmapping、license/model/telemetry選択。人の記録にもtested SHA、OS/device category、手順、観測、判定者を残す。音声や本名を自動添付しない。

## 4. Interaction / E2E を何に使うか

component interactionはdraft input、button disabled理由、歴史take表示、undo単位のUI wiringを最安で検査する。実browserではpointer capture、modifier、drag/scroll、keyboard focus、CSS/axe、route A→B→Aを検査する。mock transportはUI oracleでありnative成功を証明しない。

frontend/backend integrationはactual GraphQL document/variables/result、coherent revision、Mutation receipt/outcome、Subscription sequence/error mappingを実runtimeに接続して検査する。DB/engineをすべてmockにしない。native/Tauri integrationはgeneric GraphQL execute/subscribe/unsubscribe transport、必要ならbinary resource optimization、window lifecycle、native menus/dialog/IME/device permissionを検査する。application-specific Channel型がfeatureへ漏れていないことも検査する。

full E2Eは「新project→録音→保存→試唱→export→再open」と「旧project→migration→編集→export」の少数journeyに絞る。全てのalias collision、全crash phase、5000 bulk combinationsをmouse E2Eへ載せない。

現行英語Tauri docsはWDIO経路を含むmacOSの選択肢を案内する。一方direct tauri-driverの対応範囲とは異なる。版と実行methodをpinし、test用embedded server/pluginはtest buildだけに入れる。古い翻訳からmacOS E2E不可と断定しない。[X06]

## 5. False green を機械的に防ぐ

Target tooling では、現在の `SUITE-*` registry を **reusable Probe registry** へ移す。Probe は contract / Hypothesis / Requirement に対して「何を、どの条件・runner・fixture・backend で観測するか」を定義し、特定 run の結果とは分ける。

Probe definition には、target IDs、backend、platform、fixture/model prerequisites、minimum discovery、mandatory assertions / actual-work counters、runner command、expected skip policyを持たせる。すべてのtest binaryが0以上という単純規則ではなく、**required Probeごと**にexpected discoveryと実workを照合する。unsupportedでは0が正しいbinaryもあるが、同じreleaseのnative-positive obligationは別に残る。

GraphQL contract Probeは追加で schema hash、runtime schema hash、operations/fragments discovered/validated、subscriptions actually observed、persisted manifest entriesを Receipt に含める。0 operations/fragmentsや、subscriptionが1eventも流れていないのにgreenになることを防ぐ。

run Receipt例（提案）:

```json
{
  "probe": "alignment-real-audio",
  "git": "tested-sha",
  "backend": "mfa-native",
  "platform": "macos-arm64",
  "fixture_manifest_hash": "...",
  "discovered": 8,
  "executed": 8,
  "audio_files_opened": 8,
  "native_align_calls": 8,
  "result": "passed"
}
```

required fixture/modelが無ければexit failure。任意の手動harnessは明示 `not-run: missing fixture` を出し、CI aggregatorがpassに変換しない。LFS pointerだけで本体なしも検知する。envを設定したという事実だけでaudio読込済みとしない。source line coverageだけでは「意味ある実音声経路」を証明しない。

Receipt は generated execution artifact であり、毎回の CI result を durable `EVID-*` にしない。複数の将来判断から再利用する価値がある観測だけ、条件・限界・provenance を抜き出して Evidence に canonize する。

negative canaryは、canonical SDL/runtime schemaを1fieldずらす、GraphQL document globを空にする、persisted operationをmanifestから外す、application-specific Channel importを入れる、story globを空にする、CSSを外す、awaitを消す、traceにpath/GraphQL variablesを入れる、requiredモデルを外す、unsupported flagを注入する、migration assetを消す、という代表破壊をisolated copy上で行う。常に全mutation testingを回す必要はない。guard自体が赤くなる証拠を初回と定期的に保存する。

## 6. Performance を契約として扱う

現行 `BUDGET-*` / `TGT-*` は migration source として扱う。Target model では「破ったら product regression になる数値」は quantitative REQ、「この程度になるはず」という予測は HYP、実測値は EVID、代表 workload の選択は DEC / operational WORKLOAD に分ける。Budget の subtotal / mode peak / margin は Graph から計算する derived View であり、手書きの第二正本にしない。

現在記録されている 1500MB はprocess全体のpeakで、WebViewを含む。既存allocationsの多くは未実測である。編集chartのmode allocationは416MB、undo200MB、編集中synth128MB。Requirement の個別上限と同時使用時allocationを混同しない。初回試唱30s枠・TR-SYN-33 median1.5sをcold/warm/8phrase条件と一緒に測る。古いminiaudioや旧alignの数値はcurrent engineの実測に転用しない。

| 判断したいこと | 測定点 / workload | gateの方法 |
|---|---|---|
| callbackがdeadline内か | callback frames/sample rateからperiod、load下のduration/overrun/xrun | reference device/OS別にmissとmax/tail分布。通常callback内で重いtimer/logは使わず、検査時のみ低負荷計測をvalidatedにする |
| 音がいつ鳴ったか | UI intention→queue→render→native buffer→first audible/loopback | command returnを音が出た時刻の代用にしない。cold/warm別、中央値とworst枠 |
| memoryが実モードで収まるか | Rust/WebView/GPU関連process tree、inflight PCM/model/WASM/undo | cache件数でなくbytes、予約済みmemoryと実RSSを併記 |
| editor frameを守れるか | drag/zoom/scroll中8ms目標のwork、first waveform warm50ms/cold200ms | frame missesと長いstallを記録。平均fpsだけにしない |
| chart事前計算が速いか | 1800s audio/4core、60s requirement、途中cancel/resume | 同一dataset/hash、engine config、disk cache cold/warm |
| bulkが応答するか | 5000entries、preview200ms/commit1s要件 | DB commit、undo作成込み。pure loop microbenchのみでは不可 |
| project openが遅くなるか | 代表scaleのWAL/recovery/schema、cold filesystem | open→first useful projection、missing asset検査とlazy derived work分離 |
| queue saturationが起きるか | record postprocessing＋preview＋reestimate＋charts | depth/wait/inflight bytes、admission拒否、starvation。priorityだけで無制限queueを正当化しない |
| export/query/startupが肥大しないか | fullselectedbank/5000entries、modelcold、snapshot作成 | end-to-end duration、peakmemory、bytes読書き、DB query count |
| GraphQL execution overheadがuser-visible budgetを侵食しないか | fixed persisted operationsでApplication直呼びとの差、serialization bytes | operation latency/response bytes/snapshot latency。microbenchだけでなくroute実測 |
| Subscriptionがmemory/backpressureを壊さないか | durable/job/transient各streamのslow consumer | active subscriptions、queue bytes、coalesced/dropped、resync latency |

未登録の新しい数値は、その意味に応じて HYP / EVID として扱い、human review で hard constraint に採る場合は DEC が quantitative REQ を establish / revise する。shared hosted runnerのmicrosecond差を絶対gateにしない。通常CIは大きな退行とallocation invariantを、isolated/nightly/releaseは絶対budgetとdistributionを判定する。性能testの比較にはcommit、compiler flags、modelhash、fixture、CPU/OS、サンプル数、warmup、ばらつきを含める。

GUIが前面かどうかをmemory admissionの根拠にしない。追加consumerがrecordingとchartを同時に要求しても、runtimeがactivity/resource reservationを管理して予算を守る。必要なspec上の排他modeを明示し、view mountを排他の代わりに使わない。

## 7. Error taxonomy と caller action

以下のcodeは新contract例であり現コードに存在するとの主張ではない。

| Failure class | 代表code | Caller action | outcome |
|---|---|---|---|
| invalid input | `input.malformed`, `input.limit_exceeded` | 入力を直す。自動retryなし | not started |
| domain rejection | `capture.already_active`, `export.not_ready` | 状態/不足条件を示し操作選択 | not committed |
| stale/conflict | `project.stale_epoch`, `edit.revision_conflict` | 最新snapshot/再preview。古いmutationを自動適用しない | not committed |
| cancellation | `job.cancelled` | 通常terminalとして表示。error toast不要 | not published |
| cancelled too late | `operation.already_committed` | receiptを表示、必要なら新undo command | committed |
| resource/backpressure | `jobs.busy`, `memory.budget_unavailable` | 待機/coalesce/明示retry。録音確定と解析待ちを分離 | intent別 |
| device unavailable | `audio.disconnected`, `audio.permission_denied` | 選び直す/許可/再arm。無音を成功録音扱いしない | capture状態を併記 |
| retryable IO | `storage.temporarily_unavailable` | 同OperationIdの状態照合後にretry | not committed/unknown |
| storage full/permission | `storage.no_space`, `storage.access_denied` | 容量/権限を直す。確定assetとreceiptを保護 | phaseを併記 |
| corruption | `project.integrity_failed`, `asset.missing` | quarantine/read-only/recovery。自動正常化しない | recovery required |
| unsupported environment | `capability.unavailable`, `schema.newer_version` | 対応capabilityだけ利用/別version。無限retry禁止 | not started |
| native engine failure | `alignment.engine_failed`, `synthesis.engine_failed` | derived job retryまたはdegraded。source dataは残す | partial success可能 |
| programmer/invariant bug | `internal.invariant_violation` | 当該owner停止、sanitized diagnostic、report | 安全に継続可能な範囲を限定 |

retryableというbooleanだけでは不十分。durable outcome（not_started/not_committed/committed/unknown）、same-operation retry可否、必要user actionを明示する。unknownはまずreceipt lookup/recoveryで解決し、もう一度新captureを作ることで解決しない。GraphQLではこれらexpected application outcomesをpayload/union/objectとして表し、通常のconflict/rejection/AlreadyCommittedを `errors[]` へ変換しない。

### 7.1 Propagation

1. modelはtyped domain rejection/validation issuesを返す。invalid editor numeric draftはdocument＋violationsで受理できる。syntax malformedやunboundedexpressionは別のinvalid input。
2. native/FS/DB adapterはtyped source errorとoperation/phaseを保持する。errno/OSStatusは診断に使えるが、それだけでpublic actionを決めない。
3. application ownerがcontext（commit済みか、対象がまだ有効か、degraded可能か）を付け、public failureとinternal diagnosticを分ける。
4. GraphQL adapterはexpected outcomeをtyped payload/unionへ写し、unexpected execution failureだけをGraphQL `errors[]` にする。source chainやDebug全体は出さない。
5. generic Tauri transportはGraphQL execution result/streamを運ぶだけでapplication error classificationを作り直さない。
6. GUIはmessage keyと許可された引数からlocalizeする。外部consumerはcode/class/action/outcome typeで処理し、日本語messageをparseしない。
7. top-levelは予期しないbugをreportable execution failureへまとめられるが、sourceを無条件string化してdomain/action分類を消さない。panic catchはFFI segfault救済ではない。

GraphQL expected outcome例:

```graphql
union ApplyEditorCommandOutcome =
    EditCommitted
  | EditRevisionConflict
  | EditRejected

type EditRevisionConflict {
  code: String!
  expectedRevision: Revision!
  actualRevision: Revision!
  actions: [CallerAction!]!
}
```

内部diagnosticは同OperationIdに結び付くsource chainを保持するが、通常logへraw DisplayやGraphQL variablesを吐かない。

## 8. Trace model / realtime observability

```text
graphql.mutation FinishTake {operation_hash, consumer}
  operation.capture_finish {op, project_session, capture}
  capture.seal {frames, device_format_class}
  asset.publish {phase, result}
  project.commit {revision, result}
  -> receipt.take_committed {take, analysis_state}
  -> linked job.analysis {job, attempt, based_on}
       queue.wait
       analysis.compute {engine_version, duration}
       result.admission {accepted | stale | cancelled | protected}
       derived.publish

graphql.subscription JobEvents {job}
  subscribe.open
  event.sent / event.coalesced / event.dropped
  cursor.gap / resync
  subscribe.close {reason}
```

GraphQL operation name/hash/kindとuser intentionのOperationIdを起点にする。document全文やvariables全文はspanへ入れない。jobは親spanのスレッド生存期間に縛らずcorrelation/linkでつなぐ。project lease、capture、take、job、attempt、operationは識別可能にするが、localにopaqueな値として扱う。completionで何を捨てたかはreason enumで出し、同じmessageを何度も全stackに重複logしない。

normal loggingをcallbackに入れない。RT側はpreallocated atomic counters（frames/dropped/discontinuities/render errors/underrun）と必要なら小さなbounded diagnostic ringだけを更新する。time/counter収集もtargetでcostを測る。diagnostic ring満杯時は診断を捨ててdropped diagnosticsを数え、audioを待たせない。非RT collectorが差分・sample statisticsをaggregateする。metrics/traces backend exporterはcontrol/worker側だけで使う。

capture dropとtimestamp discontinuity、output starvation、writer backlogは別signalにする。1つのxrun counterにまとめると原因が分からない。monotonic sample positionにはstream epochを付け、新しいdevice streamの0を古い通知の遅延と取り違えない。

## 9. Metrics / privacy / diagnostics

### Engineering measurement

判断対象はdeadline・memory・queue・durability・native availabilityに加え、GraphQL snapshot/execution/serialization/subscription delivery。duration histogram、resident/inflight/cache/subscription queue bytes、response bytes、queue wait、drop/coalesce/cursor-gap counters、recovery/migration outcomes。labelsはboundedなoperation kind/backend/outcome/phaseに絞る。project/take/job IDs、filename、alias、model fullpathはlabelsにしない。job-specific追跡はtrace/eventへ。

### Product / quality measurement

alignmentの境界誤差分布・音素/話者条件別tail、制約違反率、手修正の対象と幅、保護された境界数、false accept/false review、試唱の人間評価。label付き許諾datasetとprotocol/model versionを持つ。manual correction率はUXやユーザー習熟にも依存し、そのままモデル精度と呼ばない。power-overlap sanityを20ms accuracyや知覚品質と呼ばない。[R15,R21]

### Optional telemetry

application scopeのconsent ownerを通した独立adapter。既定off、telemetry/crash別、最初のproject＋take後に訊く、撤回後はqueueと送信直前の両方で遮断。SaaSとfields/retention/locationはDEC/Qに従う。OpenTelemetry等のremote exporterをlocal tracingの自然な延長として接続しない。local測定はremote送信同意とは独立。[R17]

### Privacy rules

記録可: predefined errorcode/phase、duration、count、bytes、engine/application version、粗いbackend/OS分類。local診断ではopaque operation identifiersを使えるが、bundleでは再mapし、telemetryへは原則出さない。

通常trace/event/metrics/bundleに不可: user audio、lyrics、project/song/voicebank names、raw file path、alias本文、import原文、GraphQL variables/document本文、secrets、device serial/安定識別子、raw memory dump。単純hashにすれば匿名という判断も禁止する。allowlisted field名でも値が自由Stringなら漏れるため、typed code/enum/count constructorsを使う。

`#[instrument(err)]` や `?error` はsource Displayにpathが含まれ得る。field名のregexチェックだけで安全とはしない。source errorを安全に分類するadapter、typed event constructor、canary valuesを使う。free-text messageの送信を閉じる。

### Sanitized diagnostic bundle

含めるのはapp/build/contract/schema hash、persisted-operation manifest hash、backend availability、sanitized config categories、最近のGraphQL operation/subscription・job/recovery lifecycle、errorcodes、budget/counters、fixtureを含まない再現手順template。project DB、audio、歌詞、rawpathは既定で含めない。localでpreviewして利用者が選んで保存・共有する。自動uploadしない。個別音声の提供は別の明示的合意とライセンス確認で扱う。

## 10. Failure debugging worked trace

例: 「Aで録音できたのに、別のprojectを開いて戻ると解析が失敗と表示された」。

1. `OperationId=o1` の receipt を見る。`take_committed(t1, revision=42, analysis=pending)` があれば録音は失われていない。単なる`finish failed`では分からなかった点である。
2. linked `job=j1, attempt=1, project_epoch=7, based_on=t1/rules4/settings12` を追う。queue waitが長ければsaturationとnative compute timeを分離して見る。
3. その間の`edit_committed`でmanual pinがrevision13になり、`project.lease_closed(epoch7)`、B、Aの`lease_opened(epoch9)`がある。
4. 遅いj1は`completion.discarded(reason=stale_epoch)`でprojectへ適用されていない。epoch一致でもsettings/pinが変わればprotected/revision reasonで拒否される。discardとengine failureを混同しない。
5. 新leaseで必要なanalysisをattempt2として要求し、source asset hashとrulesが変わっていないことを確認する。take statusはsavedのまま。cancelled/stale jobのcache publicationが無いことを`cache.bytes`とpublication eventで確認する。
6. engine errorがあればsource chainをsanitized diagnostic側で追い、public APIは`analysis.engine_failed`とsaved receiptを返す。userに再録音を強制しない。

別の分岐として、`asset.publish=success`の後に`project.commit=storage.no_space`、receipt未作成なら、起動時`recovery.orphan_candidate(capture=c1)`へ到達する。FSL通り本人が選ぶまでfileを残す。`phase`とCaptureIdがあるため、stale jobと未commit WAVを混同しない。

同じevent/error/receipt contractsをapplication state-machine、fault injection、native stressでassertする。test-only log formatをもう一つ作らない。production architectureは「testを通すためのhook」ではなく、実際の回復・取消・確定責任を表し、その自然な境界をtestで使う。

## 11. Fitness functions — 強制力と維持費の区別

| Rule | 最小の強制手段 | 保証の強さ / 限界 | 維持費 |
|---|---|---|---|
| model→Tauri/SQLite/native禁止 | cargo metadataでdirect/transitive dependency allowlist、公開visibility、target build | crate edgeは強制できる。std::fs等の直接使用や隠れた副作用は別のdisallowed-api lint/reviewが必要 | 小。既存xtaskでgraphを読む |
| project writer以外からdurablemutation不可 | storeをprivate、write handleをnonclone、sealedcommit入力型 | safe APIの外へ書込権限を出さなければcompilerが守る。unsafe/別fileopenはreview対象 | 小〜中 |
| SPSC単独producer/consumer | !Sync端点、&mut操作、private split constructor、compile-fail | API取り違えをcompilerで阻止。memory orderingの正しさは別test/proof | 小 |
| feature間internalimport禁止 | 既存linterのpath restrictions、公開entrypoint | staticimportsに強い。dynamicpath/例外はreview、canaryでresolver設定漏れを確認 | 小 |
| canonical SDL ownership | canonical SDL parse/validate + runtime-exported SDL structural parity | shape driftを強制。Rust declarationとSDLの二重記述コストは残る | 中 |
| frontend operations/fragments drift禁止 | all documents validate、codegen diff、discovery receipt | data dependencyとtype driftに強い。UX意味は別test | 小〜中 |
| application-specific Channel禁止 | Cargo/import restricted rules、transport API visibility | static edgeに強い。binary最適化の例外はarchitecture review | 小 |
| subscription delivery semantics | deterministic source model＋slow-consumer integration | sequence/gap/drop/coalesceを検査。OS IPC worstcaseは実測必要 | 中 |
| persisted operation/capability | manifest completeness、unknown hash/capability rejection | production surfaceを絞れる。policy設定誤りはreview対象 | 中 |
| requiredtest0件/earlyreturn禁止 | portfolioとrunner receipts、nativecall/fixturework counters | 計測が正しく入る範囲で保証。negativecanaryが必要 | 中 |
| migrations既存互換 | versioncorpus、hash/pins/rawbytes、kill/reopen | corpus外の現実projectは保証しない。許諾fixtureの更新が必要 | 中 |
| tracing sensitivefield禁止 | typedconstructors、source scan、出口canary | arbitraryString/Debugの抜け道を完全に型で塞げない箇所は残る | 中 |
| callback noallocation/lock | restricted APIs、RT allocator guard、unsafe/callgraph review | 実行したpathに強い。OS/libraryのworstcaseを完全証明しない | 中 |
| runtime memorybudget | byte-accounted pools/reservations＋actualRSS macrobench | managedbufferは強制できる。native/WebViewの隠れallocationは測定が必要 | 中 |
| xrun/latency/quality | reference workload、native telemetry counters、ラベル/聴感 | compilerでは証明不可。環境とdataset条件付きのevidence | 大・人の検証あり |

compilerで守れるruleと、観測によって退行を検知するruleを同列に「完全保証」と呼ばない。新customtoolは既存compiler/linter/xtaskで表現できない必要最小限にし、guard自体のpositive/negative testsとmaintainerを持つ。

## 12. FSLと実装testの対応を過大評価しない

editor-constraints FSLは11制約をbooleanへ抽象化し、3msをモデル対象外とし、undoはhistory counterで表す。[R26] したがってFSL gateだけでは、sample整数のclamp、snap、bulk inverse、provenance復元、drag latencyを証明しない。FSLの離散contractをkernel/application testへ対応付け、個別数値算法にはproperty/metamorphic tests、実行時間にはbudget testを追加する。finite MAX_HISTORY=3をproductの履歴上限にしてはいけない。