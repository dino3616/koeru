# Contributor-oriented Agent Skills — 設計仕様

## 1. 境界の導出

placement、test planning、error/observability reviewを別Skillへ分断すると、各Skillが同じ変更のownerとinvariantを再推測する。配置だけ正しく、race testもpartial-success errorも無い計画が生まれる。これらは一つの「変更を設計する」workflowにまとめる。

一方、実装後のverificationは、実装者の意図から独立してdiff・実行証跡・negative casesを検査する責任を持つ。したがって**新設 `plan-koeru-change` と既存 `verify-koeru` の再設計**を推奨する。二つという数自体に価値はない。post-refactor実例でoverlapが多ければ統合またはsubworkflow化するが、同じruleを両方へコピーしない。

既存 `writing-comments` / `rust-conventions` / `react-conventions` / `setup-koeru` は即座に全廃しない。言語構文・記述・環境設定として独立したtriggerが有効なら薄く残す。architecture/error/tracingの規範がSkill内にしか無い場合はcanonical docs/config/codeへ移し、Skillから参照する。`.agents/skills` を実体、`.claude/skills` はsymlinkにする現規約を守る。[R01]

## 2. Canonical information layout

下記は提案。最終的にはpost-refactor AGENTS/architecture indexに登録された実pathを使う。

| 情報 | 正本の候補 | Skillがすること |
|---|---|---|
| product transitions/invariants | `specs/` | 関連FSLとfinite-model assumptionsを読む |
| TR/acceptedDEC/Q/profile/budget/EVID | `meta/` | confidence、supersedes、blocking question、測定条件を確認 |
| owner/dependency placement | architecture index＋machine ownership config | pathからownerを解決し、変更理由との一致を確認 |
| operational contributor rules | contributor architecture guide | decision treeを現在の変更へ適用 |
| application contract | canonical GraphQL SDL + persisted-operation/capability manifest | Query/Mutation/Subscription、public shape、evolution、capabilityを確認 |
| Rust GraphQL implementation | runtime-exported SDL + resolver dependency rules | canonical SDL parity、Application-only resolver、no domain GraphQL deriveを確認 |
| frontend data dependency | operations/fragments + generated artifacts | feature-local fragment、route operation composition、deprecated usage/codegen driftを確認 |
| test obligations | portfolio manifest＋CI config | required suite/real backend/fixtures/実行方法を解決 |
| setup/tool versions | flake/lockfiles＋setup docs | 実際の環境で実行可能か確認 |

Skillに持つのはworkflow、出力template、小さいeval fixture、canonical場所の探索規則だけ。architecture全文・予算値・errorcodes一覧・module禁止一覧をSkillへコピーしない。post-refactor pathが変わってもindexだけの更新で追える形にする。

## 3. Skill仕様: plan-koeru-change

**Skill name:** `plan-koeru-change`

**Trigger description:** KOERUの機能追加・不具合修正・設計変更について、owner、配置、lifetime、contract、test/error/observabilityを決めるとき。M6/M7仕様からimplementation taskを作るとき、cross-boundary変更のreviewを依頼されたとき。一般のRust/React質問や純粋な文章校正だけでは起動しない。

**Intended input:** 自然言語の変更要求、TR/DEC/Q ID、issue/PR URL、diff/base SHA、制約。codeがまだ無くてもよい。

**Expected output:** Change Design Card。読んだ正本とrevision、requirement/ambiguity、owner/placement、deps/public API、**Query/Mutation/Subscription/host/internal classification**、affected SDL/operations/fragments、identity/lifetime/execution、durable/derived、stale/duplicate/fault/unsubscribe scenarios、risk-test-signal matrix、human decisions、実装taskを含む。未確定specはblockedとして残す。

**Repository context it must inspect:** AGENTS、currentHEAD/status、architecture index、touched IDs、canonical GraphQL SDL、affected operations/fragments、runtime schema parity tooling、related code/direct callers、data schema、history/reviews、CI/test portfolio。branch名やfile名だけでownerを決めない。

**Canonical files it must read:** AGENTS、index、contributor guide、owner/dependency rules、関連TR/FSL/acceptedDEC/Q、canonical SDL、operation/capability manifest、該当portfolio/budget。exactpathはindexから解決。

**Optional references:** 当該PR review、実測EVID、nativevendor docs、approvedmigrationfixtures、既存workedexample。外部調査は不明な技術事実が判断を変える場合だけ。

**Required tools/connectors:** 読取可能repo、git/search、canonical validatorを読む能力。ローカルrepoが無ければGitHub read connectorでも計画はできるが実行証拠は作らない。計画段階でNix/nativehardware必須とはしない。private connectorの書込みは不要。

**Workflow:** (1) repo/commitと正本を解決 (2) touched contracts/SDL/operations/fragmentsと未決questionsを読む (3) current codeでowner仮説を検証 (4) consumer-facing changeをQuery/Mutation/Subscription/host/internalへ分類 (5) identity/lifetime/durable outcome、Subscriptionならsequence/replay/backpressure/unsubscribeを記述 (6) 最小の配置と依存を選ぶ (7) 最安の検出testとproduction signalを一緒に設計 (8) schema evolution/error/privacy/migration/capabilityをreview (9) task cardを出し、必要なhuman decisionを分離。

**Stop conditions:** 正本が無い/矛盾、FSLのhuman decisionが必要、accepted product semantics変更、旧dataの曖昧mapping、license/privacyの未承認判断。影響しない部分の計画は進められるがblocked部分を推測で確定しない。

**Checks:** 既存ownerを無視してnewcrateを提案していないか、Rust struct changeだけで不必要にSDLを変えていないか、featureが他feature private hook/Tauri Channelへ依存していないか、samealiasをIDにしていないか、unsubscribe=cancel/cancel=rollbackと誤認していないか、expected outcomeをGraphQL `errors[]`へ逃がしていないか、newfailureのcaller actionがあるか、riskにtestがあるか。

**Things it must never infer:** GUIが唯一consumer、Tauri command/Channelがapplication contractであるべき、GraphQL=HTTP/Apollo/DB resolver、mainがPRheadを含む、reviewが真実、未対応backendが動く、予算が実測、newtype/trait/crateが多いほど良い、DOCの日付が古いから無効、aliasがreadingであること。

**Example prompts:** 「M6の境界ドラッグを追加する。配置と検証計画を作って」「PR16で曲別rowを録ってもcoverageが増えない。最小の正しいownerを調べて」。

**Example output（短縮）:**

```text
Change: 曲別rowからmaterialを供給できるようにする
Contracts: current TR/FSL IDs resolved from repository
Owner: model::materials (equivalence), project writer (candidate/selection)
Placement: existing modules; new crateなし
Not an identity: alias/display row text
Tests: same target別row/別表記、same alias別tone、realSQLite selection
Signal: resolution decision + selection revision, alias本文は記録しない
Blocked: custom rules変更時のequivalence policyが未決定ならQを提示
```

**Stale architecture docs detection:** indexed paths存在、canonical SDL↔runtime SDL parity、all GraphQL documents validation、operation manifest completeness、ownergraph、DEC supersedes、touched codeにowner未登録、実dependency/importとmapの差を確認。context fingerprintにgit SHA/schema hash/operation manifest hashを記録する。hash一致は意味の正しさの証明ではなく、contradictionは具体箇所を示して止める。

## 4. Skill仕様: verify-koeru

**Skill name:** `verify-koeru`（既存Skillを更新し、重複verification Skillを増やさない）。

**Trigger description:** KOERUのdiff/PR/refactorを検証、pre-PR確認、architecture compliance、test/evidence計画を実行するとき。「CI greenだからreleaseしてよいか」の検査にも使う。変更前の配置計画だけならplan側。

**Intended input:** diff/base/target SHA、change design card、関連requirements、利用可能環境、既存CI/evidence。design cardが無い場合は必要な範囲を復元するが作者の説明だけを信じない。

**Expected output:** Verification Receipt。source-only/actually executedを分け、contract、suite、backend、fixtures、discovery/execution/pass/fail/not-run、commands、durable/errors/events、budget結果、human残件、architecture違反、testedSHAを報告する。

**Repository context it must inspect:** AGENTS、index、diff/direct callers、canonical SDL、runtime-exported SDL、GraphQL operations/fragments/codegen、persisted-operation manifest、Tauri generic transport、schema/migrations、native cfg/features、CI scripts、testsのearlyreturn/env/fixtures、canonical decisions。

**Canonical files it must read:** contributor guide、portfolio manifest、canonical SDL、operation/capability manifest、該当FSL/TR/DEC、owner/dependency rules、budget/evidence scope、diagnostic/error/privacy catalog、setup docs/lockfiles。

**Optional references:** CI job logs、review history、previousreleasefixture、nativebackend docs、human verificationrecords。PR checkboxを実行証明として採用しない。

**Required tools/connectors:** ローカルgit/filesystem/shellと正規toolchain。必要ならGitHub connectorでreview/CIを読む。nativehardwareが無ければ該当obligationをnot-runにし、mockに代替してpassにしない。connectedCIの結果と手元結果を区別する。

**Workflow:** (1) SHA/environment/schema/manifest hashesを固定 (2) touched contracts/owners/SDL/operations/fragmentsを照合 (3) risk→required suitesを解決 (4) discovery/backend/fixture/GraphQL document countsを確認 (5) SDL/runtime parity・documents/codegen・operation manifest・testsを実行 (6) subscription delivery/unsubscribe/cancelとcritical guardsのnegative casesを必要範囲実行 (7) expected outcomes/errors/traces/metrics/privacyが同じcontractを表すか確認 (8) machine receiptとhuman review残件を出力。

**Stop conditions:** destructive migrationをuserdataでしか試せない、必要fixture/license/permissionなし、missingnativebackend、actual vs documentedarchitecture矛盾、requiredguardfailure。止めた部分はblocked/not-runとし、無関係なsafe verificationは継続できる。

**Checks:** test0件、GraphQL operations/fragments discovery 0、subscription events 0、早期return、LFS pointer、wrong cfg、missing CSS/await、SDL/runtime drift、codegen/manifest drift、application-specific Channel import、resolver direct storage/native dependency、unsupported path、nativecall counters、durable outcome、privacy variables、budget実測条件、legacy production path除去。

**Things it must never infer:** test binary存在=実行、schema parse成功=runtime parity、operation validation成功=actual subscription delivery、unsubscribe=job cancel、suitepass=human聴感良好、browser mock=native integration、buildsuccess=OS対応、cancel ack=FFI停止、schema migration成功=assets完全、allowed field名=安全な値。

**Example prompts:** 「この変更をPR前に検証し、実音声が通ったかを区別して」「新storageでrename後にDBが失敗するcaseを確認して」「色だけ変えたdiffの最小verificationを選んで」。

**Example output（短縮）:**

```text
Tested: <sha>; platform: linux; backend: forced-unsupported
PASS: pure material invariants / SQLite crash-phase fixtures / contract generation
NOT-RUN: real microphone, native MFA macOS, human perceptual assessment
Finding: alignment harness discovered=0 in this cfg; native obligation未達
Guard: missing-fixture canary failed as expected
Conclusion: portable logicは検証済み。native support / release qualificationは未完了
```

**Stale architecture docs detection:** canonical SDL/runtime SDL、documents/codegen、operation manifest、ownergraph/portfolio/reference checksを先に回し、差分がある正本をreportする。feature flagのactual集合、schema hash、operation manifest hashをcontextに記録。古いverification receiptを新SHAへ流用しない。最新版というlabelより実content/hashを優先。

## 5. Skill implementation shape

各SKILL.mdは目安80–160行程度のworkflowにし、巨大architecture文書にしない。frontmatterはlowercase nameとdescriptionだけ、triggerはdescriptionに書く。`agents/openai.yaml`を付ける。referencesに正本本文をコピーせず、出力templateとeval案内程度にする。scriptは既存xtask/CIを呼ぶ薄いwrapperが本当に必要なときだけ。schemaを検査する二つ目の独自validatorをSkillに作らない。

正規のSkill creator conventionsを実装時に読む。new Skillはinit script、既存更新は既存layoutを尊重。package scriptでvalidateしてから配る。個別Skillの配布archiveは別directory内の `skill.zip`、25MB以下にする。複数Skillを一つのupload用skill.zipに束ねない。repository配布は複数directoryを持ってよい。

## 6. Representative evaluation set

| Case | 正しい振る舞い | 誤りとして落とすもの |
|---|---|---|
| row/alias reallocation | candidate/selection ownerへ、property＋DBtest | alias renameだけで修正完了 |
| M6 drag＋再推定 | kernel/parity/protected-field/admission | TSで別rule、projectrev未確認 |
| backup bug | WALconsistent snapshot＋assets＋faulttests | fs::copyだけを再配置 |
| capture finish partialfail | savedreceiptとanalysisfailureを分離 | 全rollback可能と説明 |
| trace field追加 | typedcounts/privacy＋canary | allowlistに自由Stringを追加 |
| UI stylingのみ | component/CSS/a11y中心、必要以上nativeを要求しない | 全fullE2Eを機械的に必須 |
| missingnativebackend/model | not-run/blocked | mockpassをnativepassへ変換 |
| stale index / SDL-runtime / operation manifest | drift箇所を提示し決定停止 | Rust型やfile名からcontractを捏造 |
| 無断autotranspose要求 | acceptedbehaviorと照合しproductdecisionへ | architecture都合で採用 |
| external consumer追加 | approved GraphQL operation、scope/capability/resource admission | Application Rust APIを別public contractにする/global current projectを共有 |
| streaming追加 | Subscription + owner/sequence/replay/backpressure、generic transport | application-specific Tauri Channelを追加 |
| internal Rust refactor | SDL不変ならcontract変更なし | Rust struct変更をそのままGraphQL schemaへ反映 |

検証は出力の文字列一致より、owner/contract/test/blocked判定のrubricを使う。triggerは正例だけでなく他project・一般Rust・文章校正の負例を用意する。scriptは実行してtestし、lintだけでworkflowの有効性を主張しない。