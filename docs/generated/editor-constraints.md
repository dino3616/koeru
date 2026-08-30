---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/editor-constraints.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:947cad12621c1aedae9d63f6b3bda26ebaa7d399becb4eac883e58ecaed8a8a6
claim_set_digest: sha256:dd0009d82dc5275d556f97170f9601976f465a2fd6f88e383a56677aca3606c5
---

# 要件仕様書: KoeruEditorConstraints

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

### AC-EDT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上級モードを開いても、確認状態も履歴も変わらない

（出典: `specs/requirements/editor-constraints.fsl:181`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-EDT-001#acceptance_trace" digest="sha256:a0b38509a40f323c317adb372ee1cd4402029fbea1dade3743989fdd8ba0833a" -->
#### 受け入れ基準: `AC-EDT-001`

- 識別子: `acceptance:AC-EDT-001#acceptance_trace`
- 出典: `specs/requirements/editor-constraints.fsl:181`
- 表題: 上級モードを開いても、確認状態も履歴も変わらない

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `type_value(0)`
  2. `open_advanced()`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  violating[0] and not confirmed[0] and history == 1
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-EDT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 違反を直すと確認済みになる

（出典: `specs/requirements/editor-constraints.fsl:187`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-EDT-002#acceptance_trace" digest="sha256:0d7faed25f7c17a20656f05e6819bda03955d3b9887f94069cfc416e087a448c" -->
#### 受け入れ基準: `AC-EDT-002`

- 識別子: `acceptance:AC-EDT-002#acceptance_trace`
- 出典: `specs/requirements/editor-constraints.fsl:187`
- 表題: 違反を直すと確認済みになる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `type_value(0)`
  2. `fix_violation(0)`
- 期待（Then）: 最後の操作のあと、`violating[0]` が `false` である、かつ、`confirmed[0]` が `true` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-EDT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 違反の残るエントリを通常モードで確認済みにできない

（出典: `specs/requirements/editor-constraints.fsl:193`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-EDT-001#forbidden_trace" digest="sha256:df06ce25177a742b801d2deb43a04fd13409c023fb2a0fc4357b04334d20463f" -->
#### 禁止手順: `FB-EDT-001`

- 識別子: `forbidden:FB-EDT-001#forbidden_trace`
- 出典: `specs/requirements/editor-constraints.fsl:193`
- 表題: 違反の残るエントリを通常モードで確認済みにできない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `type_value(0)`
- 期待（Then）: 続けて実行しようとする最後の操作 `confirm(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-EDT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 通常モードでは無制約編集を有効にできない

（出典: `specs/requirements/editor-constraints.fsl:199`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-EDT-002#forbidden_trace" digest="sha256:7adc0bec2c99918e592f025a529b988c3752bd57a9f661131fe18301663662e2" -->
#### 禁止手順: `FB-EDT-002`

- 識別子: `forbidden:FB-EDT-002#forbidden_trace`
- 出典: `specs/requirements/editor-constraints.fsl:199`
- 表題: 通常モードでは無制約編集を有効にできない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 先行する操作はない。初期化直後の状態で、次の操作を試みる。
- 期待（Then）: 続けて実行しようとする最後の操作 `enable_unconstrained()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-EDT-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 通常モードでは上級モードの編集をできない

（出典: `specs/requirements/editor-constraints.fsl:204`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-EDT-003#forbidden_trace" digest="sha256:2a6b8d0cb20301dd3cccb7a55d363d8a7879255630a2b7f38048f8faf5a90148" -->
#### 禁止手順: `FB-EDT-003`

- 識別子: `forbidden:FB-EDT-003#forbidden_trace`
- 出典: `specs/requirements/editor-constraints.fsl:204`
- 表題: 通常モードでは上級モードの編集をできない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 先行する操作はない。初期化直後の状態で、次の操作を試みる。
- 期待（Then）: 続けて実行しようとする最後の操作 `advanced_edit(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-EDT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 制約違反が残っているエントリは、確認済みにならない

（出典: `specs/requirements/editor-constraints.fsl:137`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ViolationIsNeverConfirmed#state_rule" digest="sha256:fb27782e9f57015f02005706d17d7406f36ac362d0588f2186b24ee76fdcc0b0" -->
#### 状態不変条件: `ViolationIsNeverConfirmed`

- 識別子: `property:invariant:ViolationIsNeverConfirmed#state_rule`
- 出典: `specs/requirements/editor-constraints.fsl:138`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `e: Entry` について、`violating[e]` が `true` であるならば、`confirmed[e]` が `false` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-EDT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 無制約編集は上級モードでしか有効にならない

（出典: `specs/requirements/editor-constraints.fsl:142`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:UnconstrainedOnlyInAdvanced#state_rule" digest="sha256:8494cb91da33c319f21f6abe7384e32c70a2d0c2684da5272227922a3ca8b119" -->
#### 状態不変条件: `UnconstrainedOnlyInAdvanced`

- 識別子: `property:invariant:UnconstrainedOnlyInAdvanced#state_rule`
- 出典: `specs/requirements/editor-constraints.fsl:143`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`unconstrained` が `true` であるならば、`mode` が `Advanced` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-EDT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-3: 履歴は検証用に有限へ閉じる

（出典: `specs/requirements/editor-constraints.fsl:147`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:HistoryBounded#state_rule" digest="sha256:b3296a40dd8994edede8e489f1efb30e35d64058dc08a33f3eb73814aa211a50" -->
#### 状態不変条件: `HistoryBounded`

- 識別子: `property:invariant:HistoryBounded#state_rule`
- 出典: `specs/requirements/editor-constraints.fsl:148`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`history` が `MAX_HISTORY` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> ドラッグは制約を破る位置で停止する。違反を作らない

（出典: `specs/requirements/editor-constraints.fsl:45`）

> ドラッグは新しい違反を作らない

（出典: `specs/requirements/editor-constraints.fsl:152`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:drag_boundary#operation" digest="sha256:f3f0dd38e8aaaa1c4efc2b6de1eaec1bf65f20c0e7f9d149d3e6ae150cb80712" -->
#### 操作: `drag_boundary`

- 識別子: `action:drag_boundary#operation`
- 出典: `specs/requirements/editor-constraints.fsl:46`
- パラメータ: `e: Entry`

操作 `drag_boundary` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `history` が `MAX_HISTORY` より小さい。
2. `violating[e]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `confirmed[e]` を `true` にする。
2. `history` を `history + 1` にする。
3. `last` を `Drag` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:DragNeverCreatesViolation#transition_rule" digest="sha256:519252348eb7b7d4c52c3c804c17efbb194e67f4a9014a003e607904b7c29acd" -->
#### 遷移条件: `DragNeverCreatesViolation`

- 識別子: `property:trans:DragNeverCreatesViolation#transition_rule`
- 出典: `specs/requirements/editor-constraints.fsl:153`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
last == Drag => (forall e: Entry { violating[e] => old(violating[e]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> インポートも制約違反を許す

（出典: `specs/requirements/editor-constraints.fsl:63`）

> 数値の直接入力は制約違反を許す。該当エントリは警告表示になる

（出典: `specs/requirements/editor-constraints.fsl:54`）

> 無制約編集では、違反を残したまま上級モードで編集できる

（出典: `specs/requirements/editor-constraints.fsl:176`）

> 無制約編集は上級モードだけが持つ

（出典: `specs/requirements/editor-constraints.fsl:122`）

> 直接入力で違反が残る状態が生じうる

（出典: `specs/requirements/editor-constraints.fsl:171`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:enable_unconstrained#operation" digest="sha256:43de9e3f8ce0e8d8b68430c4c0396ea4f3ef066f22d494cc3a31555f05b370ee" -->
#### 操作: `enable_unconstrained`

- 識別子: `action:enable_unconstrained#operation`
- 出典: `specs/requirements/editor-constraints.fsl:123`
- パラメータ: なし

操作 `enable_unconstrained` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `mode` が `Advanced` である。
2. `unconstrained` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `unconstrained` を `true` にする。
2. `last` を `AdvancedEdit` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:import_oto#operation" digest="sha256:7c87a4368ef2b0d21aa5e6b0ffb87913a94a390b9daa50eb12e647e799366757" -->
#### 操作: `import_oto`

- 識別子: `action:import_oto#operation`
- 出典: `specs/requirements/editor-constraints.fsl:64`
- パラメータ: `e: Entry`

操作 `import_oto` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `history` が `MAX_HISTORY` より小さい。
2. `violating[e]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `violating[e]` を `true` にする。
2. `confirmed[e]` を `false` にする。
3. `history` を `history + 1` にする。
4. `last` を `Import` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:type_value#operation" digest="sha256:1fccb54ea98c3de9a7770275b95c076e49e3e12d5c01086ee641fb07b865baf6" -->
#### 操作: `type_value`

- 識別子: `action:type_value#operation`
- 出典: `specs/requirements/editor-constraints.fsl:55`
- パラメータ: `e: Entry`

操作 `type_value` を実行できるのは、次の条件を満たす場合に限る。

1. `history` が `MAX_HISTORY` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `violating[e]` を `true` にする。
2. `confirmed[e]` を `false` にする。
3. `history` を `history + 1` にする。
4. `last` を `TypeValue` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:UnconstrainedKeepsViolation#reachability_goal" digest="sha256:bc4c4a13f7712f6b86128eeb56b04baf92f2d3edfb7783a7b59e63e00a5a153f" -->
#### 到達目標: `UnconstrainedKeepsViolation`

- 識別子: `property:reachable:UnconstrainedKeepsViolation#reachability_goal`
- 出典: `specs/requirements/editor-constraints.fsl:177`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
unconstrained and (exists e: Entry { violating[e] })
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ViolationCanRemain#reachability_goal" digest="sha256:4533986d2af267ab8fd4d583e5e312a4157f96aba6040833decba163263b2525" -->
#### 到達目標: `ViolationCanRemain`

- 識別子: `property:reachable:ViolationCanRemain#reachability_goal`
- 出典: `specs/requirements/editor-constraints.fsl:172`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `e: Entry` が存在して、`violating[e]` が `true` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 違反を直せば、確認待ちから外れる

（出典: `specs/requirements/editor-constraints.fsl:73`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:fix_violation#operation" digest="sha256:5574a2d0777420d19851d41734e8b986d54afb6c70a8c74187d50733386efbe9" -->
#### 操作: `fix_violation`

- 識別子: `action:fix_violation#operation`
- 出典: `specs/requirements/editor-constraints.fsl:74`
- パラメータ: `e: Entry`

操作 `fix_violation` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `violating[e]` が `true` である。
2. `history` が `MAX_HISTORY` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `violating[e]` を `false` にする。
2. `confirmed[e]` を `true` にする。
3. `history` を `history + 1` にする。
4. `last` を `Fix` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 履歴を1件戻せる

（出典: `specs/requirements/editor-constraints.fsl:130`）

> 通常モードの確認操作も1件の操作として履歴に積む

（出典: `specs/requirements/editor-constraints.fsl:83`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:confirm#operation" digest="sha256:1d102b98d242588ccc9bd318def30a46f5f11a860813e672b7104d829f0c53bf" -->
#### 操作: `confirm`

- 識別子: `action:confirm#operation`
- 出典: `specs/requirements/editor-constraints.fsl:84`
- パラメータ: `e: Entry`

操作 `confirm` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `mode` が `Normal` である。
2. `violating[e]` が `false` である。
3. `confirmed[e]` が `false` である。
4. `history` が `MAX_HISTORY` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `confirmed[e]` を `true` にする。
2. `history` を `history + 1` にする。
3. `last` を `Confirm` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:undo#operation" digest="sha256:3f9f56b4b0f22f11400481fbc3f9355be391c7034ccc866210099a9ceb623af8" -->
#### 操作: `undo`

- 識別子: `action:undo#operation`
- 出典: `specs/requirements/editor-constraints.fsl:131`
- パラメータ: なし

操作 `undo` を実行できるのは、次の条件を満たす場合に限る。

1. `history` が `0` より大きい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `history` を `history - 1` にする。
2. `last` を `Undo` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上級モードで編集したエントリは確認済みになる。ただし違反が残るなら確認待ちのまま

（出典: `specs/requirements/editor-constraints.fsl:94`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:advanced_edit#operation" digest="sha256:a47b45115a10da40680319ab723a8a6181525690057d937b784a7ca5eb9696b4" -->
#### 操作: `advanced_edit`

- 識別子: `action:advanced_edit#operation`
- 出典: `specs/requirements/editor-constraints.fsl:95`
- パラメータ: `e: Entry`

操作 `advanced_edit` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `mode` が `Advanced` である。
2. `history` が `MAX_HISTORY` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `confirmed[e]` を `if unconstrained then not violating[e] else true` にする。
2. `violating[e]` を `if unconstrained then violating[e] else false` にする。
3. `history` を `history + 1` にする。
4. `last` を `AdvancedEdit` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-006

**要件原文（意図。形式意味との一致は人間が確認する）**

> モードの切り替えは、エントリの状態も履歴も変えない

（出典: `specs/requirements/editor-constraints.fsl:157`）

> 上級モードは1操作で開ける。開いてもプロジェクトの状態は変わらない

（出典: `specs/requirements/editor-constraints.fsl:106`）

> 通常モードへ戻る操作も1操作

（出典: `specs/requirements/editor-constraints.fsl:114`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:close_advanced#operation" digest="sha256:413b85df2a8e7ad15001fb274cc86cc7a6cb2ab726d15d92254096e8ab6676bf" -->
#### 操作: `close_advanced`

- 識別子: `action:close_advanced#operation`
- 出典: `specs/requirements/editor-constraints.fsl:115`
- パラメータ: なし

操作 `close_advanced` を実行できるのは、次の条件を満たす場合に限る。

1. `mode` が `Advanced` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `mode` を `Normal` にする。
2. `unconstrained` を `false` にする。
3. `last` を `CloseAdvanced` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:open_advanced#operation" digest="sha256:ef590b458bd2c562c78e3c5b2546a188841d52f48c42c90bb865bdeac51964fe" -->
#### 操作: `open_advanced`

- 識別子: `action:open_advanced#operation`
- 出典: `specs/requirements/editor-constraints.fsl:107`
- パラメータ: なし

操作 `open_advanced` を実行できるのは、次の条件を満たす場合に限る。

1. `mode` が `Normal` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `mode` を `Advanced` にする。
2. `advanced_ever_opened` を `true` にする。
3. `last` を `OpenAdvanced` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ModeSwitchChangesNothing#transition_rule" digest="sha256:427b2cd33363065d4e0389a1fda307fdc6c51686550e6048843cc42f89b12455" -->
#### 遷移条件: `ModeSwitchChangesNothing`

- 識別子: `property:trans:ModeSwitchChangesNothing#transition_rule`
- 出典: `specs/requirements/editor-constraints.fsl:158`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
last == OpenAdvanced or last == CloseAdvanced => history == old(history) and (forall e: Entry { violating[e] == old(violating[e]) and confirmed[e] == old(confirmed[e]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-EDT-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上級モードを一度も開かずに、すべてを確認済みにできる

（出典: `specs/requirements/editor-constraints.fsl:166`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:CompletableWithoutAdvanced#reachability_goal" digest="sha256:a81e37d0a7b971ef87d95f7a4dae462af3792a3defdc72a04df1ee6ed51b4516" -->
#### 到達目標: `CompletableWithoutAdvanced`

- 識別子: `property:reachable:CompletableWithoutAdvanced#reachability_goal`
- 出典: `specs/requirements/editor-constraints.fsl:167`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall e: Entry { confirmed[e] }) and (forall e: Entry { not violating[e] }) and not advanced_ever_opened
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:4d7783487e778cc7881739a6031fb96411e9101f24c3afac7f8a392d11bff583" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall e: Entry { confirmed[e] }) and (forall e: Entry { not violating[e] }) and mode == Normal
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `Entry` の解析インスタンス数: 2
- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/editor-constraints.fsl`（`KoeruEditorConstraints`、dialect: `requirements`）
- spec digest: `sha256:947cad12621c1aedae9d63f6b3bda26ebaa7d399becb4eac883e58ecaed8a8a6`
- claim set digest: `sha256:dd0009d82dc5275d556f97170f9601976f465a2fd6f88e383a56677aca3606c5`
- 形式要素の分類: rendered 23 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 6 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 6 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
