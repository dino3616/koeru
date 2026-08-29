---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/align-review.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:1fd9a04f3baae5ecac4360e890aaca74cde3d0a611cbcacbb50e8ca51d0e83ab
claim_set_digest: sha256:5fde3de4a70303cc3c3d1cd1ebd789c79cb90918929fb8ab7dc90d4dcaca54ba
---

# 要件仕様書: KoeruAlignReview

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

### FB-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認待ちが残ったまま書き出せない

（出典: `specs/requirements/align-review.fsl:201`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-001#forbidden_trace" digest="sha256:a8242644afc72c3db104136baac9cf9b14208fa21c1b69a710a8568e1abb1cc7" -->
#### 禁止手順: `FB-ALN-001`

- 識別子: `forbidden:FB-ALN-001#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:201`
- 表題: 確認待ちが残ったまま書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `estimate_confident(0)`
  2. `estimate_low_confidence(1)`
- 期待（Then）: 続けて実行しようとする最後の操作 `export()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-ALN-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 修復できない違反が残ったまま書き出せない

（出典: `specs/requirements/align-review.fsl:208`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-002#forbidden_trace" digest="sha256:6bbfe92c6c9a351220745b51047f5c9c3e0c7db4775abadb3e7489597b2b0aac" -->
#### 禁止手順: `FB-ALN-002`

- 識別子: `forbidden:FB-ALN-002#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:208`
- 表題: 修復できない違反が残ったまま書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `estimate_confident(0)`
  2. `estimate_confident(1)`
  3. `validation_unrepairable(1)`
- 期待（Then）: 続けて実行しようとする最後の操作 `export()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-ALN-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上限を超えていないのに個別確認をやめられない

（出典: `specs/requirements/align-review.fsl:216`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-003#forbidden_trace" digest="sha256:a0d96565573a99ccea19d6a45444bf69ffdcf7f2f1c5b77c3d3f08ef96d9b7c5" -->
#### 禁止手順: `FB-ALN-003`

- 識別子: `forbidden:FB-ALN-003#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:216`
- 表題: 上限を超えていないのに個別確認をやめられない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `estimate_confident(0)`
  2. `estimate_confident(1)`
- 期待（Then）: 続けて実行しようとする最後の操作 `budget_exceeded_to_batch()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 自動の再推定は、固定された値に触れない

（出典: `specs/requirements/align-review.fsl:171`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:AutoPassNeverTouchesPinned#transition_rule" digest="sha256:427777515c47b28bfa26a6c33c3843234ce8baef58ad913ab0645298fd669338" -->
#### 遷移条件: `AutoPassNeverTouchesPinned`

- 識別子: `property:trans:AutoPassNeverTouchesPinned#transition_rule`
- 出典: `specs/requirements/align-review.fsl:172`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
auto_pass => (forall s: Slot { old(pinned[s]) => source[s] == old(source[s]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 固定されていることと、人が入れた値であることは同じ

（出典: `specs/requirements/align-review.fsl:166`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:PinnedIffHuman#state_rule" digest="sha256:39822b99d0b900d8c584b5fec594643d58fb9e943b79e6f8178f61f1cd556163" -->
#### 状態不変条件: `PinnedIffHuman`

- 識別子: `property:invariant:PinnedIffHuman#state_rule`
- 出典: `specs/requirements/align-review.fsl:167`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `s: Slot` について、`pinned[s]` が `source[s] == Human` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認が残っているものがあるうちは、書き出されていない

（出典: `specs/requirements/align-review.fsl:176`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoExportWhileReviewPending#state_rule" digest="sha256:7a11ddfa7d700a9443e51c485ba8c6c6deb6e38600597d976c30157aa9928f5a" -->
#### 状態不変条件: `NoExportWhileReviewPending`

- 識別子: `property:invariant:NoExportWhileReviewPending#state_rule`
- 出典: `specs/requirements/align-review.fsl:177`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
exported => (forall e: Entry { entry[e] == AutoConfirmed })
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 個別確認をやめるのは、上限を超えたときだけ

（出典: `specs/requirements/align-review.fsl:181`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ModeChangeNeedsBudget#state_rule" digest="sha256:8d1850d37daa4a66b5e7ea2fa5da4d861126893a193d020905e97a37619be789" -->
#### 状態不変条件: `ModeChangeNeedsBudget`

- 識別子: `property:invariant:ModeChangeNeedsBudget#state_rule`
- 出典: `specs/requirements/align-review.fsl:182`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`mode` が `Individual` でないならば、`over_budget` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確信度が足りていれば、自動推定した値をそのまま確定させる

（出典: `specs/requirements/align-review.fsl:53`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:estimate_confident#operation" digest="sha256:b8770fadafabc9c7a09f1a65619812c4202f7c95deccba068f946aad09aab5a9" -->
#### 操作: `estimate_confident`

- 識別子: `action:estimate_confident#operation`
- 出典: `specs/requirements/align-review.fsl:54`
- パラメータ: `e: Entry`

操作 `estimate_confident` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `NotEstimated` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `AutoConfirmed` にする。
2. `auto_pass` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確信度が足りない項目は自動確定させず、確認キューへ回す

（出典: `specs/requirements/align-review.fsl:61`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:estimate_low_confidence#operation" digest="sha256:baef914d4b5a785e8aad0477d6db5c96165c1a7cf1ca4e965d58d35283c649a7" -->
#### 操作: `estimate_low_confidence`

- 識別子: `action:estimate_low_confidence#operation`
- 出典: `specs/requirements/align-review.fsl:62`
- パラメータ: `e: Entry`

操作 `estimate_low_confidence` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `NotEstimated` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `InQueue` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> テキスト逸脱と判定したテイクは oto を自動確定させず、確認キューへ回す

（出典: `specs/requirements/align-review.fsl:69`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:text_deviation#operation" digest="sha256:bbfa7b4f450c5d06bf81eb9e9bf49300339804e227efe577c8db95452c0d8792" -->
#### 操作: `text_deviation`

- 識別子: `action:text_deviation#operation`
- 出典: `specs/requirements/align-review.fsl:70`
- パラメータ: `e: Entry`

操作 `text_deviation` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `NotEstimated` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `InQueue` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 修復できない違反で書き出しが止まる状態が生じうる

（出典: `specs/requirements/align-review.fsl:196`）

> 書き出し前の検証で修復できない違反があれば、確認キューへ回して書き出しを止める

（出典: `specs/requirements/align-review.fsl:77`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:validation_unrepairable#operation" digest="sha256:374644e6c8218c3c1d52eebef0f25da120856087d35322b8aae3952dfc51b606" -->
#### 操作: `validation_unrepairable`

- 識別子: `action:validation_unrepairable#operation`
- 出典: `specs/requirements/align-review.fsl:78`
- パラメータ: `e: Entry`

操作 `validation_unrepairable` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `AutoConfirmed` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `Blocked` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ExportBlocked#reachability_goal" digest="sha256:a03525be67eed3b19395487420e6de2e1a5e0ddf79ad9b63ee3bb4f2003be920" -->
#### 到達目標: `ExportBlocked`

- 識別子: `property:reachable:ExportBlocked#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:197`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `e: Entry` が存在して、`entry[e]` が `Blocked` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上級モードで人が編集した値は、値単位で固定する

（出典: `specs/requirements/align-review.fsl:85`）

> 外部ツールで変わった値は、次回読み込み時に固定として取り込む

（出典: `specs/requirements/align-review.fsl:103`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:human_edit#operation" digest="sha256:5abb9f3144fcd7da9a0975c578b8f8a87b95efbf27349d0ea6bf0d28ccb2790d" -->
#### 操作: `human_edit`

- 識別子: `action:human_edit#operation`
- 出典: `specs/requirements/align-review.fsl:86`
- パラメータ: `s: Slot`

操作 `human_edit` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `exported` が `false` である。
2. `entry[s / VALUES]` が `NotEstimated` でない。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `true` にする。
2. `source[s]` を `Human` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:import_external_edit#operation" digest="sha256:c598b6a847af5c59e921043f678e354e4e522c2e680bc6cb92bafd6ec4aa586b" -->
#### 操作: `import_external_edit`

- 識別子: `action:import_external_edit#operation`
- 出典: `specs/requirements/align-review.fsl:104`
- パラメータ: `s: Slot`

操作 `import_external_edit` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `pinned[s]` が `false` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `true` にする。
2. `source[s]` を `Human` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-006

**要件原文（意図。形式意味との一致は人間が確認する）**

> 固定を解くのは、本人が明示的に「自動に戻す」を選んだときだけ

（出典: `specs/requirements/align-review.fsl:94`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:revert_to_auto#operation" digest="sha256:8477c845a8b0a87f64ccbfa61e0fb59b437dc3c707d3d2fcb922ce50966480a5" -->
#### 操作: `revert_to_auto`

- 識別子: `action:revert_to_auto#operation`
- 出典: `specs/requirements/align-review.fsl:95`
- パラメータ: `s: Slot`

操作 `revert_to_auto` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `pinned[s]` が `true` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `false` にする。
2. `source[s]` を `Auto` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> 再推定しても、固定した値は残る

（出典: `specs/requirements/align-review.fsl:186`）

> 再推定は、固定されていない値だけを書き換える

（出典: `specs/requirements/align-review.fsl:112`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:re_estimate#operation" digest="sha256:ac7686b7943c4bbca29d69c9c64f53dff2f72f8c21ca80714e9324afa96805bc" -->
#### 操作: `re_estimate`

- 識別子: `action:re_estimate#operation`
- 出典: `specs/requirements/align-review.fsl:113`
- パラメータ: `e: Entry`

操作 `re_estimate` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `NotEstimated` でない。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. すべての `s: Slot` について、次を適用する。

   1. `source[s]` を `if s / VALUES == e and not pinned[s] then Auto else source[s]` にする。
2. `auto_pass` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ReEstimateKeepsPinned#reachability_goal" digest="sha256:00004cc05f5c11f415556f62237565f120e79a3eee28507e22d1eda43471a073" -->
#### 到達目標: `ReEstimateKeepsPinned`

- 識別子: `property:reachable:ReEstimateKeepsPinned#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:187`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次のすべてが成立する。

1. `pinned[s]` が `true` である。

2. `source[s]` が `Human` である。

3. `entry[s / VALUES]` が `AutoConfirmed` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-008

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認キューの項目は、人が確認すれば確定する

（出典: `specs/requirements/align-review.fsl:122`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:confirm#operation" digest="sha256:b3c20475a7c58c37a8cdd2de7e1bcc4bf477639f77155f3378be3df1250d90fd" -->
#### 操作: `confirm`

- 識別子: `action:confirm#operation`
- 出典: `specs/requirements/align-review.fsl:123`
- パラメータ: `e: Entry`

操作 `confirm` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `InQueue` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `AutoConfirmed` にする。
2. `auto_pass` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-009

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認から録り直しへ回る経路が存在する

（出典: `specs/requirements/align-review.fsl:191`）

> 確認キューの項目は、oto の修正ではなく録り直しを選べる

（出典: `specs/requirements/align-review.fsl:130`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:rerecord#operation" digest="sha256:b1d718a6ea87eb8834c405dafbcdc9071ae99e2ba522c19a6504f826b6e04044" -->
#### 操作: `rerecord`

- 識別子: `action:rerecord#operation`
- 出典: `specs/requirements/align-review.fsl:131`
- パラメータ: `e: Entry`

操作 `rerecord` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `InQueue` である、または、`entry[e]` が `Blocked` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `NotEstimated` にする。
2. `auto_pass` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RerecordFromQueue#reachability_goal" digest="sha256:9f7ac234e33babea01c556b15e246d2e8d878f6df8ecb90f0fe3c5cfba62765d" -->
#### 到達目標: `RerecordFromQueue`

- 識別子: `property:reachable:RerecordFromQueue#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:192`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(exists e: Entry { entry[e] == NotEstimated }) and over_budget
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-010

**要件原文（意図。形式意味との一致は人間が確認する）**

> 上限を超えたとき、録り直し提案へ切り替えることもできる

（出典: `specs/requirements/align-review.fsl:148`）

> 確認の合計が上限を超えたら、個別確認をやめてまとめて確認か録り直し提案へ切り替える

（出典: `specs/requirements/align-review.fsl:139`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:budget_exceeded_to_batch#operation" digest="sha256:bf0965b2efb7db078a33adc0c8fc58c75233e94cac07f8777f11317702da3f71" -->
#### 操作: `budget_exceeded_to_batch`

- 識別子: `action:budget_exceeded_to_batch#operation`
- 出典: `specs/requirements/align-review.fsl:140`
- パラメータ: なし

操作 `budget_exceeded_to_batch` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `over_budget` が `false` である。
2. ある `e: Entry` が存在して、`entry[e]` が `InQueue` である、または、`entry[e]` が `Blocked` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `over_budget` を `true` にする。
2. `mode` を `Batch` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:budget_exceeded_to_rerecord#operation" digest="sha256:2bc97a13c327e6d1933626077832fe28042da954ef367e9f705b3e8a9bf6e917" -->
#### 操作: `budget_exceeded_to_rerecord`

- 識別子: `action:budget_exceeded_to_rerecord#operation`
- 出典: `specs/requirements/align-review.fsl:149`
- パラメータ: なし

操作 `budget_exceeded_to_rerecord` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `over_budget` が `false` である。
2. ある `e: Entry` が存在して、`entry[e]` が `InQueue` である、または、`entry[e]` が `Blocked` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `over_budget` を `true` にする。
2. `mode` を `SuggestRerecord` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認が残っている間は書き出せない

（出典: `specs/requirements/align-review.fsl:157`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export#operation" digest="sha256:8e081e5ce2e2a8f891b917308df381153436cac14f1e68f9096d586e6310a3d4" -->
#### 操作: `export`

- 識別子: `action:export#operation`
- 出典: `specs/requirements/align-review.fsl:158`
- パラメータ: なし

操作 `export` を実行できるのは、次の条件をすべて満たす場合に限る。

1. 0. 次の条件（FSL canonical 形式で示す）:

   ```fsl
   not (exists e: Entry { entry[e] == InQueue or entry[e] == Blocked })
   ```。
2. すべての `e: Entry` について、`entry[e]` が `AutoConfirmed` である。
3. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `exported` を `true` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:80d32c8ba46199695bbd701391f4b64357b2548a196495bfbfff59bbd05acc2b" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

`exported` が `true` である。

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/align-review.fsl`（`KoeruAlignReview`、dialect: `requirements`）
- spec digest: `sha256:1fd9a04f3baae5ecac4360e890aaca74cde3d0a611cbcacbb50e8ca51d0e83ab`
- claim set digest: `sha256:5fde3de4a70303cc3c3d1cd1ebd789c79cb90918929fb8ab7dc90d4dcaca54ba`
- 形式要素の分類: rendered 23 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 4 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 4 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
