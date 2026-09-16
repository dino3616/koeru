---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/method-coverage.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:202ea5127c06be95c66e80086f8c8f7576fd86309a3853d9d36dbfa151976398
claim_set_digest: sha256:557a3cb596d8b62abb775201f06c7b6f9fbae8f6c926843e6b52455a9a0bb814
---

# 要件仕様書: KoeruMethodCoverage

<!-- fsl:slot begin name="background" normative="false" -->
## 背景

（この節は自由に編集できる。規範的な効力はない。規範文はこの節の外の生成ブロックにのみ存在する。）
<!-- fsl:slot end -->

## 本書の位置づけ

本書の「形式化された意味」は、検査済みの FSL 仕様から決定論的に生成した規範文である。同じ仕様からは、常にバイト単位で同一の文書が生成される。

本書が保証するのは、FSL が検査した構造 — 実行条件、更新、事後条件、否定、公平性、期限、範囲 — を欠落なく決定論的に表示することである。本書は、日本語の文と FSL の式が意味的に同値であることを証明するものではない。また、FSL が元の業務意図を正しく捉えていることも保証しない。要件原文と形式化された意味との一致の確認は、人間のレビューに委ねられる。

規範文は claim の種類ごとの固定テンプレートで生成しており、一義性を流暢さより優先している。

## 全体の意味規約

本仕様のすべての操作に、次の実行規約が適用される。

- 更新はステップ単位で同時にコミットされる（`updates: simultaneous`）。
- 更新の右辺は遷移前の状態を読む（`reads: pre_state`）。
- 事後条件・状態不変条件・遷移条件のいずれかに違反するステップはコミットされず、状態は遷移前のまま残る（`failed_step: rollback`）。
- 公平性の仮定は弱い公平性である（`fairness: weak`）。公平性は `fair` と宣言された操作にのみ適用される。

## 検証結果の読み方

- `proved(induction)`: k帰納法により全深さで証明済み。
- `bounded(BMC depth k)`: 深さ k までのすべての実行を検査した。それ以遠の実行については何も保証しない。これは証明ではない。
- `replay-observed`: 具体的な実行ログ／トレースを仕様と照合した結果のみ。すべての実行に対する保証ではない。
- `statistical`: 統計的裏付け（Wilson 区間）。個別の実行に対する保証ではない。
- `not_run`: 対応するエビデンスは供給されていない。

- 保証クラスは検証手法の網羅範囲を表し、合否を表さない。反例が存在する場合もクラスは `bounded` のままである。合否は各エビデンスの結果欄に別掲する。
- `fair` は公平性という検証上のスケジューリング仮定であり、即時実行の保証ではない。
- `leadsTo`（進行条件）は将来のある時点での成立を要求する demand であり、それ自体は検証結果ではない。
- refinement は safety の保存を検査するものであり、liveness を自動的には保存しない。
- エビデンスが供給されていない側面は、省略されず `not_run` と明示される。

本書は要求内容（what）を定義する。検証がどこまで実施されたか（how far）は `fslc ledger` が生成する監査台帳が示す。両者は同じ保証クラス語彙を用いる。

## 要件

### AC-PKG-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上位で録った素材から下位のパッケージを出せる

（出典: `specs/requirements/method-coverage.fsl:97`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PKG-103#acceptance_trace" digest="sha256:703014a9c9a40976918de463b58e6370a987b88c53eab6cce04f8a48f31b6e9e" -->
#### 受け入れ基準: `AC-PKG-103`

- 識別子: `acceptance:AC-PKG-103#acceptance_trace`
- 出典: `specs/requirements/method-coverage.fsl:97`
- 表題: 上位で録った素材から下位のパッケージを出せる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `declare_required(0, 0)`
  2. `declare_required(0, 1)`
  3. `declare_required(1, 0)`
  4. `finish_declaring()`
  5. `record(0)`
  6. `record(1)`
  7. `export(1)`
- 期待（Then）: 最後の操作のあと、`exported[1]` が `true` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 要求エイリアスが揃わない方式は書き出せない

（出典: `specs/requirements/method-coverage.fsl:108`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-104#forbidden_trace" digest="sha256:feb7afa1b63c405f29ea449d1bfa457f571d6ea5fd5d38c592b6938bd91c249a" -->
#### 禁止手順: `FB-PKG-104`

- 識別子: `forbidden:FB-PKG-104#forbidden_trace`
- 出典: `specs/requirements/method-coverage.fsl:108`
- 表題: 要求エイリアスが揃わない方式は書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `declare_required(0, 0)`
  2. `declare_required(1, 0)`
  3. `declare_required(1, 1)`
  4. `finish_declaring()`
  5. `record(0)`
- 期待（Then）: 続けて実行しようとする最後の操作 `export(1)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録した方式が要求しないエイリアスは録れない

（出典: `specs/requirements/method-coverage.fsl:118`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-105#forbidden_trace" digest="sha256:6edefd71590ad2f27b155636ee6cb029a4b1ffb2793ae25cbade4a27d4a84c50" -->
#### 禁止手順: `FB-PKG-105`

- 識別子: `forbidden:FB-PKG-105#forbidden_trace`
- 出典: `specs/requirements/method-coverage.fsl:118`
- 表題: 収録した方式が要求しないエイリアスは録れない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `declare_required(0, 0)`
  2. `finish_declaring()`
- 期待（Then）: 続けて実行しようとする最後の操作 `record(1)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録した方式そのものは、全部録れば必ず書き出せる

（出典: `specs/requirements/method-coverage.fsl:87`）

> 書き出しは、要求エイリアス表が確定したあとにしか起きない

（出典: `specs/requirements/method-coverage.fsl:77`）

> 被覆していない方式のパッケージは存在しない

（出典: `specs/requirements/method-coverage.fsl:70`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ExportedIsCovered#state_rule" digest="sha256:834c10a9692fee708b79f41b6820afbd4ee9c3391ce10b9dedc46b3f08bdf373" -->
#### 状態不変条件: `ExportedIsCovered`

- 識別子: `property:invariant:ExportedIsCovered#state_rule`
- 出典: `specs/requirements/method-coverage.fsl:71`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall m: Method { exported[m] => (forall a: Alias { required.contains(m, a) => provided.contains(0, a) }) }
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:ExportedOnlyAfterDeclaring#state_rule" digest="sha256:58db0180920c07a9ff93b4615aa1de5c3603e3bfc4a98ed4abee17f20fd420be" -->
#### 状態不変条件: `ExportedOnlyAfterDeclaring`

- 識別子: `property:invariant:ExportedOnlyAfterDeclaring#state_rule`
- 出典: `specs/requirements/method-coverage.fsl:78`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `m: Method` について、`exported[m]` が `true` であるならば、`declared` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RecordedMethodBecomesExportable#reachability_goal" digest="sha256:cf28b35ab98493508e02cb9675d5338cb746c4149ae5620268c0db11c088852d" -->
#### 到達目標: `RecordedMethodBecomesExportable`

- 識別子: `property:reachable:RecordedMethodBecomesExportable#reachability_goal`
- 出典: `specs/requirements/method-coverage.fsl:88`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
declared and (forall a: Alias { required.contains(0, a) => provided.contains(0, a) })
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録した方式が要求しないエイリアスは、決して手に入らない

（出典: `specs/requirements/method-coverage.fsl:82`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ProvidedIsSubsetOfRecorded#state_rule" digest="sha256:029e79e3a0ac800b412e888d3a1ccf700df3180e2885365315c1eabf334411be" -->
#### 状態不変条件: `ProvidedIsSubsetOfRecorded`

- 識別子: `property:invariant:ProvidedIsSubsetOfRecorded#state_rule`
- 出典: `specs/requirements/method-coverage.fsl:83`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall a: Alias { provided.contains(0, a) => required.contains(0, a) }
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-108

**要件原文（意図。形式意味との一致は人間が確認する）**

> 方式が要求するエイリアス表はプリセットが決める。収録の前に確定する

（出典: `specs/requirements/method-coverage.fsl:42`）

> 表が確定したら収録に入る。以後は要求側が動かない

（出典: `specs/requirements/method-coverage.fsl:48`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:declare_required#operation" digest="sha256:3e448a6710cfc5658db5338b8174b5d97ed9b392659f0c10ad9e317585ec386e" -->
#### 操作: `declare_required`

- 識別子: `action:declare_required#operation`
- 出典: `specs/requirements/method-coverage.fsl:43`
- パラメータ: `m: Method`、`a: Alias`

操作 `declare_required` を実行できるのは、次の条件を満たす場合に限る。

1. `declared` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `required` を `required.add(m, a)` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:finish_declaring#operation" digest="sha256:9e7e51bd1dadbd5596df84a37a40b4e3fee6592e8f1edfd58de6b3295a35e572" -->
#### 操作: `finish_declaring`

- 識別子: `action:finish_declaring#operation`
- 出典: `specs/requirements/method-coverage.fsl:49`
- パラメータ: なし

操作 `finish_declaring` を実行できるのは、次の条件を満たす場合に限る。

1. `declared` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `declared` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-109

**要件原文（意図。形式意味との一致は人間が確認する）**

> 下位方式は、上位の素材だけで書き出せる

（出典: `specs/requirements/method-coverage.fsl:92`）

> 書き出せるのは、要求エイリアスが全て揃っている方式だけ

（出典: `specs/requirements/method-coverage.fsl:62`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export#operation" digest="sha256:85e8ede70bd275b1fa03f286e06580f9b6e118cf045c658c786bff08425ce602" -->
#### 操作: `export`

- 識別子: `action:export#operation`
- 出典: `specs/requirements/method-coverage.fsl:63`
- パラメータ: `m: Method`

操作 `export` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `declared` が `true` である。
2. 0. 次の条件（FSL canonical 形式で示す）:

   ```fsl
   forall a: Alias { required.contains(m, a) => provided.contains(0, a) }
   ```。
3. `exported[m]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `exported[m]` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:DowngradeWithoutExtraRecording#reachability_goal" digest="sha256:97af288f02357b667c1237e07772b0d1b0a1de8753a07f4ac1ee92ffb23a8769" -->
#### 到達目標: `DowngradeWithoutExtraRecording`

- 識別子: `property:reachable:DowngradeWithoutExtraRecording#reachability_goal`
- 出典: `specs/requirements/method-coverage.fsl:93`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `m: Method` が存在して、`m` が `0` に等しくない、かつ、`exported[m]` が `true` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録るとエイリアスが増える。収録した方式が要求するものだけが手に入る

（出典: `specs/requirements/method-coverage.fsl:54`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:record#operation" digest="sha256:fe3b7340ce1a20fe5998428b0de4cab8758bf5a9600459040314302313365fb5" -->
#### 操作: `record`

- 識別子: `action:record#operation`
- 出典: `specs/requirements/method-coverage.fsl:55`
- パラメータ: `a: Alias`

操作 `record` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `declared` が `true` である。
2. 0. 次の条件（FSL canonical 形式で示す）:

   ```fsl
   required.contains(0, a)
   ```。
3. 0. 次の条件（FSL canonical 形式で示す）:

   ```fsl
   not provided.contains(0, a)
   ```。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `provided` を `provided.add(0, a)` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:aa0c32b990fcd9aabf0347fe4af3cccb0823b7f07ae3e122a36ad0a3450dc1c8" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
declared and (forall m: Method { exported[m] or not (forall a: Alias { required.contains(m, a) => provided.contains(0, a) }) })
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `Method` の解析インスタンス数: 2
- エンティティ `Alias` の解析インスタンス数: 2

## 生成情報

- 生成元仕様: `specs/requirements/method-coverage.fsl`（`KoeruMethodCoverage`、dialect: `requirements`）
- spec digest: `sha256:202ea5127c06be95c66e80086f8c8f7576fd86309a3853d9d36dbfa151976398`
- claim set digest: `sha256:557a3cb596d8b62abb775201f06c7b6f9fbae8f6c926843e6b52455a9a0bb814`
- 形式要素の分類: rendered 12 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 7 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 7 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
