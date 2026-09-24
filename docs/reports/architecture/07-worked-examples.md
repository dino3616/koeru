# Worked examples — 要件から owner / test / diagnosis まで

以下は提案architectureでの変更の通し方である。結果・数値を実行済みとするものではない。各例の根拠は `06-evidence.md` を参照。

## A. 曲別に詰め直したrowを録音してもcoverageが増えない

**1. Requirement / change:** PR16の `repack_for_selection` が作る短いrowも録音素材を供給できるようにする。TR-RCL-16の用途と、出力の一意性を両立する。現在はcanonical rowsのalias claimが新rowのalias保存を抑える。[R05]

**2. Owner:** semantic targetのequivalenceと候補解決はmodel::materials。candidateを保存しselectionを変えるのはproject writer。

**3. Placement:** 既存material moduleとstorage relationを変更。reclist feature専用の別coverage実装を作らない。row_aliasesを唯一の所有権表として使わず、row→target contribution、take/span→candidate、target→selectionを分ける。

**4. Dependency:** reclistとsongplanとpreview/exportは同じresolverを使い、用途による許容条件だけ渡す。frontendはcoverage projectionを読むだけ。

**5. State / lifetime:** target/row/take/candidateはproject durable。selection revisionで切替。表示row orderはmutable projectionで、candidate identityに影響しない。

**6. External/API exposure:** `recording_plan`はrow IDと要求target、`coverage`は不足/充足理由、`select_candidate`はCandidateIdとexpected revision。aliasをAPIの主鍵にしない。

**7. Tests:** canonical rowとrepacked rowを逆順に登録してもsemantic coverageは同じ。2候補/1selection、same alias別tone、未録音canonicalが録音済みrepackedを妨げない、renameでtake履歴が消えない。実SQLiteでも同じ制約を検証し、preview/export planの解決を確認。

**8. Error handling:** 複数候補は正常。採用policyが不明ならexplicit ambiguous-selection/needs-reviewであり、silently dropではない。古いselection revisionはconflict。

**9. Tracing:** GraphQL operation name/hash→OperationId→capture/take→candidate registration→selection revision。alias本文/variables全文でなくopaque IDsとreason enum。

**10. Metrics:** unresolved target count、candidate conflict count、projection時間。candidateごとのIDをmetric labelにしない。

**11. Human verification:** 実際に短い曲別rowを録って試唱/配布に寄与すること、既存canonical素材の選択が予期せず変わらないこと。custom rules変更でのequivalenceは未決なら人へ。

## B. 録音は保存されたが後続解析が失敗した

**1. Requirement / change:** 現finishはDB take commit後に解析/FRQ/alignment等を続ける。利用者が「録れなかった」と誤解せず、保存済みmasterを失わず再解析できるようにする。[R06,R10]

**2. Owner:** AudioHost/capture ownerがstreamをseal、storageがasset publish、project writerがtake/receiptをcommit、JobRuntimeが解析を実行。

**3. Placement:** capture use caseとstorage protocolを分ける。F0 estimateをproject writer lock内に置かない。

**4. Dependency:** capture completionはsealed FinalizedAssetをprojectへ渡す。analysisはimmutable take fingerprintを入力とし、完了時だけownerへ戻す。

**5. State / lifetime:** CaptureIdは開始〜recovery、TakeIdはdurable、JobId/attemptは解析寿命。projectを閉じてもmasterは残り、旧epochの解析適用は拒否する。

**6. External/API exposure:** GraphQL `finishTake` Mutationは`receipt`とtyped `FinishTakeOutcome`を返し、`TakeCommitted{analysis: PENDING|FAILED|READY}`をdurable successとして表す。`job(id)` Query / `jobEvents(job)` Subscription / `retryAnalysis` Mutationを分ける。返答喪失時は同OperationIdで同receiptを得る。

**7. Tests:** filewrite/sync/rename/DB fail、commit直後response drop、analysis error、queue full、duplicatefinish、A→B→A、sameoperation異payload。realSQLite/FS＋kill/reopen。human orphan adoption以外で自動takeを作らない。

**8. Error handling:** commit前のstorage failureとcommit後のanalysis failureを別outcomeにする。unknownはreceipt lookup、orphansはrecovery。analysis retryで新takeを作らない。

**9. Tracing:** `FinishTake` operation/hash→OperationId→capture.seal→asset.publish→project.commit→take_committed→linked analysis job。`JobEvents` Subscriptionのopen/close/event countsも相関させる。fsync/rename/DB phaseが残る。

**10. Metrics:** capture seal/commit latency、analysis queue wait、inflight bytes、recovery候補数、inputdrop。callbackとstorageのlatencyを混ぜない。

**11. Human verification:** mic入力・音頭・tailを聴く。diskfullで保存状況が誤表示されないか、復旧候補が本人の判断まで残るか。realpowerloss保証はOS/filesystem qualificationが別途必要。

## C. M6で境界をドラッグ中、古い再推定結果が戻る

**1. Requirement / change:** TR-EDT-01/04/20/21/25/43–46。absolute sample、snap、位置保持mode、1gesture1undo、数値/importのinvalid許容、manual/confirmed保護を同じmodelで実現する。[R15]

**2. Owner:** model::editingが変換と制約。runtime::editing/project writerが現在revision・provenance・historyを確定。Reactはgesture/focus、WASMはpreview。

**3. Placement:** purekernelを新機能ではなく既存editing ownerに置く。frontend editorに別の5値business validatorを作らない。

**4. Dependency:** React pointer→sample proposal→WASM kernel。releaseでapplication apply-edit→native samekernel→commit。再推定は同entry/boundaryのBasedOnを持つ。

**5. State / lifetime:** drag中draftはview/editor session。EntryIdとper-boundary provenanceはdurable。pending analysisはinputstamp/attempt、undoはproject editing history。

**6. External/API exposure:** `applyEditorCommand` Mutationにentry/baseRevision/action/parametersを渡す。editor fragmentは必要fieldだけ宣言する。drag previewそのものを毎frame GraphQL/Tauri往復にしない。automationもapproved GraphQL operationを通り同じaction制約に従う。

**7. Tests:** negative overlap、minimumgap、snap windowなし/あり、modifier mode、numericinvalid、unconstrainedadvanced、oneundo、native/WASM parity。再推定completionをdrag前/commit前/後へ並べ替え、手編集が消えないことを確認。browser/nativeでIMEとpointer capture。

**8. Error handling:** revisionconflictなら最新snapshotと再適用候補。invalidnumericはdocument+issuesとして保持し、render/exportのみ必要条件で拒否。stale jobはengine failureにしない。

**9. Tracing:** gesture完了の`ApplyEditorCommand` operationだけtrace、毎pointermoveやGraphQL variables全文はtraceしない。job admissionのprotected/revision reasonを関連付ける。

**10. Metrics:** gesture frame work、commit latency、tile cache bytes、conflict/protected counts。8ms等の既存budgetを代表scaleで確認。

**11. Human verification:** 普通の修正が自然か、snapが聴覚的境界に適切か、screenreader/focus/IME、normal/advancedで同じデータを見ているか。pitchmark詳細が未決なら比較検証とproduct decision。

## D. M6の条件選択＋フィールド式による一括編集

**1. Requirement / change:** TR-EDT-27–30。12種に加えた13番目の制限された式、20件before/after preview、1回のundo、5000entriesの性能。任意JS runtimeの導入ではない。[R15]

**2. Owner:** pure editing transformとbounded expression evaluatorがpolicy。project editingが対象集合・revision・historyを所有。

**3. Placement:** 既存bulk command ownerへ追加。新plugin crateやScriptServiceを作らない。named presetsはtyped parameters/ASTを保存し、コード実行権限を持たない。

**4. Dependency:** UIはselector/expressionを作り、backendが解釈・validation・planを返す。既存12操作は同じ内部command pathのnamed formにする。

**5. State / lifetime:** preview tokenは対象IDs・base revisions・parameters hashを固定し短い寿命。commitされたchangesetとinverseはdurable/history。巨大対象のdiffはchunk化しても1つのuseroperation。

**6. External/API exposure:** pure previewなら bounded `previewBulkEdit` Query、resource/jobを作る実装なら `startBulkEditPreview` Mutation + `jobEvents` Subscription。commitは`commitBulkEdit(token, operationId)` Mutation。式grammar/演算予算をschema-backed contractにし、任意eval()を公開しない。

**7. Tests:** selectorの集合一致、式のdeterminism/overflow/zero divide/depth/size、inverse property、blank/0、5000件実DB、preview後の並行edit、重複commit、historybytes cap。

**8. Error handling:** invalidexpressionとdomain制約違反を分離。preview後に対象が変わったらconflictで再preview。手編集保護のdefaultは採択済みpolicyに従い、件数を返す。

**9. Tracing:** bulk.preview/commitにcountsとoperation/token correlation。expression本文やaliaspredicate文字列をtraceへ送らない。

**10. Metrics:** preview/commit duration、affected/protected/rejected count、undo bytes、maximum evaluator work。個々のentryIDはlabelにしない。

**11. Human verification:** 20件previewで実際の影響が理解できるか、意図しない広範囲編集を防げるか、oneundoで心理的安全性があるか。必須集合の不足はlocal quality measurementとQへ。

## E. 外部OTOの非破壊importと外部変更監視

**1. Requirement / change:** TR-EDT-01/36–41。unknown lines、blank/0、encoding、cutoffsign/WAV lengthを保存し、無編集roundtripで1byteも変えない。external editは由来/確認/scoreを既定policyで更新する。[R15]

**2. Owner:** formatsがlossless syntax、runtime importerが解釈とasset resolution、editing ownerが変更の意味、watcher adapterが外部変化検出。

**3. Placement:** raw document modelとsemantic entriesを別に持つ。getterやproject openの副作用で外部fileを正規化しない。

**4. Dependency:** watch event→content fingerprint→ownoperation判定→diff plan→本人のimport/ignore/compare。formatparserはmanual confirmationを決めない。

**5. State / lifetime:** original bytes/sign/lengthはproject durable。watcherはproject/export-folder lease。external change proposalはbase fingerprint付き。ownexport receiptsは再検知抑制の根拠。

**6. External/API exposure:** import preview/change proposalをGraphQL Query projection、本人の選択をMutationで表す。external watcherのlong-lived通知はSubscription、authoritative stateはQueryで再取得する。rawpathを万能read権限として公開しない。

**7. Tests:** exactbytes corpus、unknownlines同位置、blank/0、CP932とUTF-8、WAV length変化、nestedfolders、ownwrite vs externalwrite、rename/retry、malformed/hugearchive。型が解釈できない行と危険なcontainer入力を別扱い。

**8. Error handling:** unknownlineは保持＋issueで扱える。encoding不明はsilentguessで完了にしない。WAV長変更はabsolute/endrelativeの本人選択を要求。missingassetはcorruption/recoveryまたはimportdiagnostic。

**9. Tracing:** import/compare/commitのoperationとsafe counts、external_change/own_write_suppressed。歌詞/alias/原文を記録しない。

**10. Metrics:** inputbytes/entries/headersread、changed/protected counts、importduration、watchercoalescing。品質側にはどのfield群が一括で動いたかのlocal集計を残す。

**11. Human verification:** setParam/vLabeler等での実往復、文字化け/丸めがないか、外部編集後の「未確認」表示が納得できるか。外部WAV変換条件は未確定なら勝手に決めない。

## F. 下位方式へ配布してもprojectの素材・編集を壊さない

**1. Requirement / change:** PR16の方式拡張と下位方式export。source方式とexport target方式、候補選択とファイル表現を分離する。[R02,R24]

**2. Owner:** material resolver/export planningが何を出せるかを判断。formatsが外部表現、runtime exporterがasset stagingとpublish。

**3. Placement:** 旧packageのpolicyはmodel、codecはformats、filesystem orchestrationはruntimeへ。audio deviceやalign engineに依存するexport policyを作らない。

**4. Dependency:** currentselection/settings/rulesのsnapshot→immutable ExportPlan→codec/write。必要deriveddataはjobsで事前解決し、export実行中にactive settingsを別解釈しない。

**5. State / lifetime:** planは入力revisionsを固定。outputはrelease artifact、receiptはdurable。projectmaster/editedentryをexport都合でrename/rewriteしない。

**6. External/API exposure:** `prepareExport` Mutationはresource/planningを開始する場合plan token/jobを返し、read-onlyな既存plan確認はQuery。`commitExport` Mutationがreceipt。進捗は`jobEvents`/export Subscriptionで観測し、dry-runはproject truthを書き換えない。

**7. Tests:** sharedresolverの用途差、samealias別tone、ending等の既存behavior、downmapping、outputcollision、nonmutatingpreflight、deterministicbytes、rename/DBfailure、externaltoolparity。

**8. Error handling:** material不足はnot-ready、invalidencodingはuseraction、staleplanはreprepare、publish後receipt失敗はoutcomeunknown→reconcile。二重出力を避ける。

**9. Tracing:** export.intent→plan→assetstage→validate→publish→receipt。source/targetmethodはbounded enums、歌詞/pathは出さない。

**10. Metrics:** exportduration/bytes/peakmemory、missingmaterial counts、plan/query time。project完成率とexport可否を同一metricにしない。

**11. Human verification:** 実UTAU/OpenUtau consumerで読め、意図した声が鳴ること。downmappingの音質と編集への影響を確認し、仕様が曖昧ならproductdecision。

## G. M7の同意撤回とqueued telemetry

**1. Requirement / change:** DEC-TEL-001、Q-TEL-001、telemetry-consent FSL。defaultoff、telemetry/crash別、project作成と最初のtake後に一度だけ訊く。SaaS選定は未決。[R17]

**2. Owner:** application-level ConsentStore/state machine。TelemetryAdapterが現在のconsentを送信直前にも確認する。project DBやReact routeではない。

**3. Placement:** runtime consent/diagnosticsとdesktop consent interaction。generictracingexporterをそのままSaaSへ接続しない。

**4. Dependency:** first-use facts→application consent、local typed events→whitelist projection→consentgate→selectedSaaS。localdiagnosticsはconsentによらず利用可。

**5. State / lifetime:** consentはapplication durable、project switch/削除/新規projectで複製しない。送信queueはkindとconsentrevision付きで、撤回時に無効化する。

**6. External/API exposure:** GraphQL Query/Mutationでconsent stateを読む/変更するが、telemetry payload delivery自体はGraphQL contractではなくTelemetryAdapter。automationがGUI非表示だからgrant済みとしない。crashreport consentは別field/capability。

**7. Tests:** 未質問/未録音送信拒否、askonce、independentgrant/revoke、enqueue後revoke、send直前revoke、apprestart、projectA/B、typedfield privacycanary、bundle default。

**8. Error handling:** consentなしはnormal suppressionでerror toastなし。SaaS unavailableはlocal操作を妨げない。privacy schema違反はbug/blocked送信。retryで撤回を無視しない。

**9. Tracing:** consenttransitionとsafe drop reasonはlocal、payload内容はtraceしない。telemetry送信を表すeventがさらにtelemetryを無限生成しない。

**10. Metrics:** local schema-reject/consent-suppressed counts、粗いoperation quality aggregates。project/take IDsやuser audioを外へ出さない。

**11. Human verification:** 同意画面が録音の邪魔をしないか、何をどこへ送るか理解できるか。SaaS保持期間/所在地/fieldsとpolicy一致を人が承認。未選定ならactualsend実装・M7qualificationはblockedのまま。

## Future consumer stress test

MCPから「project Aを読み、bulk previewし、本人確認後に適用、exportする」場合でも、上記D/Fと同じcanonical GraphQL operationsをapproved/persisted operationとして実行する。MCPだけが`Application::...`を直接呼ぶ別public contractは作らない。新しいのはMCP mapping、authorization/filesystem scope、confirmation policy、capability/resource quotaである。GUIと同時に動くならOperationId/ProjectLease/revisionで競合を解決しwriterを守る。untrusted pluginのsandbox/ABIは別future decisionであり、今plugin frameworkを作る必要はない。

## H. 20Hz入力メーターを application-specific Channel なしで流す

**1. Requirement / change:** 収録中のinput envelope/peakを低遅延に表示する。ただしstreamingだけRust/Tauri固有contractへ戻さず、consumer-visible streamをGraphQL Subscriptionへ統一する。

**2. Owner:** audio callbackはRT sample producer、pump/control側がbounded `InputEnvelopeObservation` を作り、GraphQL Subscription adapterがconsumer projectionを行う。Tauriはgeneric execution-stream transportのみ。

**3. Placement:** RT codeは`koeru-audio`、subscription root/typeは`koeru-graphql`、generic Channel bridgeは`koeru-desktop`、rAF storeはrecording feature。application-specific `Channel<InputEnvelopeFrame>`は作らない。

**4. Dependency:** callback→preallocated SPSC→pump→bounded observation source→`inputEnvelope` Subscription→generic Tauri stream→feature-local external store。RT callbackからGraphQL/JSON/tracingを呼ばない。

**5. State / lifetime:** InputLease/stream epochがsource lifetime。subscription instanceはobserver lifetimeであり、unsubscribeはInputLease/captureを自動停止しない。frame sequenceはdrop/gap観測用、replayなし。

**6. External/API exposure:** `subscription InputMeter($input: InputLeaseRefInput!) { inputEnvelope(input:$input) { sequence positionSamples peak envelope droppedSamples } }`。production operation manifestにcapability/costを登録する。

**7. Tests:** slow consumerでbounded/latest-wins、sequence gap、rapid subscribe/unsubscribe、project switch、old stream epoch、generic Tauri roundtrip、featureからChannel import禁止、RT allocator guard。actual event count 0をgreenにしない。

**8. Error handling:** observer closeはnormal terminal。device disconnectはtyped application/audio stateとしてSubscription/Queryに現れ、GraphQL execution errorへしない。transport failureはresubscribeし、durable stateが必要ならQueryで再同期する。

**9. Tracing:** subscription operation name/hash、open/close reason、sent/coalesced/dropped、queue bytes、stream epoch。meter値そのものをnormal telemetryへ送らない。

**10. Metrics:** RT input dropped/discontinuityとSubscription observation dropped/coalescedを別counterにする。GraphQL execution/serialization/IPC latencyを測るがcallback deadlineと混同しない。

**11. Human verification:** native WebViewでmeterが滑らかで録音停止/画面遷移に干渉しないこと。20Hz payload/copy costが問題ならbinary最適化を測定後検討するが、stream lifecycle contractはSubscriptionに残す。
