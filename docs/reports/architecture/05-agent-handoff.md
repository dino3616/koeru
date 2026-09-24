# Standalone instruction: implement KOERU contributor Agent Skills after architecture refactoring

## Mission

あなたは、architecture refactoringが完了したKOERU repositoryを受け取るcoding agentである。この指示書以外の会話を知らなくてよい。目的は、初見contributor/agentが新しい変更のplacement、ownership、tests、errors、observability、pre-PR verificationを再現可能に判断するためのAgent Skillsを**実装・検証・package**すること。

KOERUは、録音から歌声ライブラリ編集・試唱・配布までを扱うlocal desktop applicationである。実際のlanguage/crate/path/API/CIは必ず受領repositoryで確認する。過去の名前や本指示書の候補名から実装構造を推測しない。production architectureを再設計するtaskではない。

## Input / authority

入力はpost-refactor repositoryのroot、対象branch/commit、利用可能なshell/toolchain/connectors。編集可能範囲は原則Skills、Skill metadata/evaluations、canonical docsへの入口リンク、必要な小さいworkflow wrapperのみ。production codeやproduct semanticsの変更が必要と判明した場合は問題を報告し、その修正をこのtaskへ紛れ込ませない。

dirty working treeを保存する。無断のgit reset/clean、ユーザーデータでのmigration、録音/音声収集、外部送信、credentials使用をしない。レビューやdocumentationに記載された未検証commandをそのまま実行せず、作業範囲と安全性を確認する。

## Canonical hierarchy

最初にroot `AGENTS.md`、`README.md`、`CONTRIBUTING.md`を読む。次にそこからarchitecture index/contributor guide/ownership rule/test portfolio/**canonical GraphQL SDL**/operation manifest/error/diagnostic schemaの**実際のpath**を解決する。

KOERUでは、FSL `specs/` が状態・遷移・不変条件の正本、`meta/` がtechnical requirements、accepted decisions、open questions、profiles/milestones、budgets、evidenceの正本である。canonical `schema.graphql` は consumer-facing application vocabulary/capability の正本であり、FSL/metaの意味論を上書きしない。Markdownは背景/判断ガイド。生成文書は手で編集しない。product visionの確定方針を尊重する。Skill本文はarchitectureの正本ではない。

現実のindexがこの説明と矛盾する場合、具体箇所と採択記録を照合する。最新accepted decisionが更新しているならそれを使う。説明不能な矛盾があればblockedとし、独自のarchitecture規則を発明しない。

番号付きDEC/Q等を追加する必要があるなら、repositoryのID allocation commandを読む。既存IDを上書きしない。Skill内の擬似番号をcanonical IDとして使わない。

## Deliverables

実体のSkill directoryとSKILL.md、必要な`agents/openai.yaml`、小さいtemplates/evaluations、実行済みvalidation結果、代表変更と負例のevaluation結果、canonical reference map、個別のvalidated `skill.zip`を作る。canonical情報をコピーした巨大reference集は作らない。

既存規約が存続していれば `.agents/skills/` に実体を置き、`.claude/skills/` はsymlinkのみ。受領repoで必ず確認する。archiveは `dist/<skill-name>/skill.zip` のように個別directoryへ出力し、upload用一つのskill.zipに複数Skillsをまとめない。各archiveは25MB以下。

## Step 1 — Read and validate post-refactor context

1. git SHA/status、workspace/package/feature/cfg、canonicalindexのversionを記録する。
2. architecture guide、owner/dependency map、canonical SDL、runtime-exported SDL/parity tooling、GraphQL operations/fragments/generated artifacts、persisted-operation/capability manifest、error/action/diagnostic定義、test portfolio/CI、関連setup/lockfilesを読む。
3. FSL/metaからidentity、durability、manual-edit protection、job admission/cancellation、telemetry/privacy、M6/M7 obligationsを必要範囲だけ読む。
4. canonical SDL parse/validation、runtime SDL parity、all GraphQL documents/codegen、operation manifest、ownergraph/reference検査の実commandを解決し、利用できる範囲を実行する。実行不能なら理由を記録し、passにしない。
5. existing Skillsのname、trigger、内容、参照先を一覧化し、canonical rulesがSkillにだけ存在していないか確認する。

**Stop:** architecture refactoringが未完了、index欠落、ownerとcodeが矛盾、canonical SDL/runtime SDL、documents/codegen、operation manifestがdriftしたままのとき。問題一覧と安全に進められる部分を提示する。file名から正本を推測して穴埋めしない。

## Step 2 — Derive Skill boundaries and triggers

異なるworkflowを先に分ける。

- 変更前/設計review: 意図→FSL/meta→Query/Mutation/Subscription/host/internal分類→canonical SDL/operations/fragments→owner→placement/deps/lifetime→risk/test→error/observability→tasks。
- 実装後/pre-PR: diff→SDL/runtime parity→operations/fragments/manifest discovery→required obligations→actual backend/fixtures→execution/subscription delivery→negative guards→evidence/remaining human work。

推奨candidateは新設 `plan-koeru-change` と既存 `verify-koeru` の更新。ただし実repoのcontributor workflowで検証し、重複したarchitecture-review/test-plan/observability Skillを量産しない。placement・tests・errorsを同じchange cardで扱うのが高凝集かを確認する。既存language/style/setup Skillsは固有用途を残し、architecture本文を削って正本への参照に変える。

trigger matrixを作る。正例: KOERU機能追加、M6editor、identity bug、durability、pre-PR。負例: 他repo、一般Rust質問、文章校正、単純な使い方説明。曖昧な「Rustを書くとき全部」をnew architecture Skillのtriggerにしない。

## Step 3 — Implement using current Skill conventions

実行環境のSkill creator guidanceを読む。new Skillは提供されるinit scriptを使い、既存Skillは構造を保って更新する。利用できるscriptのpath/argumentsを確認してから実行し、記憶から絶対pathを決めない。

各Skillに以下を実装する:

- YAML frontmatterはlowercase-hyphen nameとdescription。descriptionが役割とtriggerを明示する。
- `agents/openai.yaml`は現在のschemaに従う。
- bodyはimperativeなworkflow。canonical docsを読む順序、change card/verification receiptを出す手順、stops/checksを記す。
- canonical本文はコピーしない。budget数値、errorcodes一覧、crates一覧、GraphQL fields/capabilities、禁止import一覧は正本から読む。
- output templateとevaluation rubricは小さいreferenceに分ける。deep reference nestingを作らない。
- scriptsは既存xtask/CIの薄いwrapperが必要なときだけ。architecture validatorをSkill側へ複製しない。
- examples/TODO/未使用assetsを除去。repository全体、user audio、model binaries、secrets、font filesをpackageしない。

目安はSkill本文80–160行程度。行数を満たすために不明瞭に略さない。500行を超えるarchitectureの説明文にしない。

## Step 4 — Required workflow outputs

### Plan output

```text
Context: tested/read SHA + canonical references/hashes
Change intention / contract IDs / unresolved questions
Decision owner / commit owner / resource owner
Placement / dependency / public API rationale
Query / Mutation / Subscription / host / internal classification
Canonical SDL + affected operations/fragments + schema evolution
Identity / lifetime / execution / durable vs derived
Stale / cancel / duplicate / partial failure / unsubscribe / sequence-gap behavior
Risk -> cheapest test -> higher confirmation -> production signal
Error class / caller action / trace correlation / measurement / privacy
Migration and human decisions
Concrete tasks and acceptance criteria
```

### Verification output

```text
Target SHA / environment / actual backend / fixture hashes
Touched contracts and architecture compliance
Each required suite: planned / discovered / executed / passed / failed / not-run
Commands, exit status, real-work counters, evidence paths
Canonical SDL/runtime parity + GraphQL documents/codegen/manifest + dependency/portfolio drift results
Fault/race/negative-canary results
Performance scope and measurements (not unmeasured estimates)
GraphQL expected-outcome vs execution-error / subscriptions / receipts / privacy checks
Human/native/perceptual obligations outstanding
Conclusion limited to evidence actually obtained
```

GraphQL expected application outcomesを`errors[]`へ押し込まない。Subscription unsubscribeをapplication cancelと解釈しない。AppErrorの表示文字列をparseして判断しない。test binaryの存在を実行証跡としない。browser mockをnative成功へ読み替えない。予算の存在を実測達成としない。cancel ackをnative処理停止・durable rollbackの証拠としない。

## Step 5 — Validate with representative real changes

受領repositoryのactualdiff/history/fixturesから、次の性質の異なる変更を選ぶ。存在しないものはisolatedworktreeの小さいdraft changeとして明示し、実在した履歴と偽らない。

| Case | Planで必要な判断 | Verifyで必要な証拠 |
|---|---|---|
| 同じsemantic targetが別row/alias表記から得られる | target/candidate/selectionの区別 | pureproperty＋realDB＋用途resolver |
| M6drag中に再推定が完了 | oneeditingkernel、protectedfields、inputrevision | native/WASM parity、completion順序入替、gesture |
| WAV確定後にDB失敗、またはWAL snapshot | partialsuccess、storageprotocol、manualrecovery | realFS/SQLite、fault/kill/reopen、assetmanifest |
| trace/errorfield追加 | typedcalleraction、safecontext、privacy | canaryvalueが全出口で漏れない |
| project A→B→A | leaseepoch、consumer context、coherent GraphQL read revision | delayed query/subscription/mutation、stale discard、cursor resync |
| UIの見た目だけ | frontend-local owner、過剰抽象なし | CSS/component/a11y中心。不要なnativefullE2E強制なし |
| new streaming requirement | Subscription owner、sequence/replay/backpressure、unsubscribe≠cancel | generic Tauri transport roundtrip、slow consumer、no feature Channel import |
| Rust internal struct refactor | SDL unchanged unless application language changes | runtime SDL parity、consumer docs unchanged |

評価はplanとverifyを独立に実行し、rubricで判定する。望ましい文章の丸暗記ではなく、owner選択・blocked判定・requiredtests・根拠の正しさを評価する。既存workedexamplesを丸ごとSkillへ埋め込まず、評価結果にどの正本を読んだか記録する。

## Step 6 — Negative cases are mandatory

最低限、以下でSkillが誤ったgreen/決定を出さないことを検証する。

1. required real-audio fixture/model/LFS本体が無い。
2. supportedを名乗るplatformで実際はunsupported backend、またはtest collectionが0。
3. canonical SDLとruntime-exported SDL、GraphQL documents/codegen/operation manifest、owner mapがactual codeと食い違う。
4. 新しいGraphQL field/variables/error extensionsにrawpath/lyrics/free-text errorを載せようとする。
5. product decisionなしに自動移調・orphan自動採用・telemetry既定onへ変えようとする。
6. 同じOperationIdで異なるpayload、cancel後のlatecompletionを正常適用しようとする。
7. unit/browser mockが全greenだがnative/human gateが未実施。
8. unrelated cosmetic changeに全migration/nativeE2Eを要求して過剰検証する。
9. review commentが古く、指摘箇所は既に修正されている。
10. application-specific Tauri Channelを「高速だから」という理由だけで追加しようとする。
11. Subscription unsubscribeでbackground jobをcancelしようとする。
12. internal Rust struct変更だけなのにSDLを連動変更しようとする。
13. MCP consumerがGraphQL executorを迂回してApplication Rust APIを別public contractとして使おうとする。
14. canonical documentを読めない環境でfile名だけからownerを推測しようとする。

evaluationのためにfixtureやvalidatorを壊す場合はisolatedcopy/worktreeを使う。ユーザーのmain working treeや実project dataを壊さない。意図的な失敗を実装の退行と混同しない。

## Step 7 — Detect stale docs and avoid rule drift

各runでcanonical pathsの存在、canonical SDL/runtime SDL hash/parity、GraphQL document/codegen/operation manifest hashes、DEC supersedes/ref coherence、ownergraph、feature/cfgとportfolioの一致を確認する。context fingerprintをrecordする。documentationの日付だけではstaleと判断しない。hashが一致しても意味の正しさは保証されないので、差分のactualownershipとの矛盾を具体箇所で検査する。

Skillにcanonical命題をコピーした箇所を検索し、どうしても必要な短いworkflow判断以外は参照に直す。error catalog/予算/GraphQL generated artifactsは再生成先を一つにする。修正がSkill本文だけに存在してrepository正本に反映されていない状態を合格にしない。

矛盾を見つけたらreportに「document path + code path + observed contradiction + affected decisions + safe next action」を出す。Skillが独断で新architectureへ書き換えない。

## Step 8 — Validate, package, install-smoke, report

現在のSkill validatorとpackage scriptを実際に実行する。一般形は `scripts/init_skill.py <name> --path <dir>`、`scripts/package_skill.py <skill-dir> <dist-dir>` だが、利用環境の実pathとschemaを先に読む。new Skillのみinit、updateは既存を保つ。

validation errorsを修正し、失敗archiveを配布しない。wrapper scriptsは実行testし、単に構文checkだけで済ませない。個別Skill archiveを必ず `skill.zip` として作り、SHA256、size、file listを記録する。zip内に絶対path/親directory traversal、不要symlink、secret、userdata、model/audio/font、repo丸ごとコピーが無いことを確認する。

別のclean temporary locationに展開し、canonicalrepoを指定して入口・references・script pathsが解決するinstall smokeをする。Skill単体だけではrepository正本を持たないことを明記する。複数Skillをrepositoryで配る場合も、個別upload archiveを分ける。

最終報告には、選んだSkill boundaryと理由、trigger matrix、canonical reference map、変更一覧、正例/負例evaluation結果、validator/package/installログ、実行不能/残るhuman確認、個別archiveの場所を含める。実施していないnative/聴感/production変更を完了としない。

## Completion criteria

初見agentがこの指示書とpost-refactor repositoryだけで、FSL/meta/SDL正本の読取→Query/Mutation/Subscription分類→workflow実装→代表change検証→negative case→schema/operation drift確認→packageまで行える。Skillsを削除してもarchitectureのcanonical rulesはrepositoryに残る。Skillsが読めない/矛盾している状態を推測で埋めず、未実行をpassにせず、日常の小変更に過剰なframework/testを強制しない。この三点を評価で確認して完了とする。