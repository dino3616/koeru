---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/align-review.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:50a00022ad8e27a1fc46a4c63286a5018f85d1f3dea3e725d5e3d12d66159d55
claim_set_digest: sha256:dcc1920e0b59fae1edeeac699cb69110093e94e2b0f0e755fe9cbaf8600ae6f6
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

### AC-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 再推定は、固定した値を残し、固定していない値だけを書き換える

（出典: `specs/requirements/align-review.fsl:230`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-ALN-001#acceptance_trace" digest="sha256:0bdf7960963863207ad92c3fb74a00a56ccad4055c58155a14dfd309d291ff0b" -->
#### 受け入れ基準: `AC-ALN-001`

- 識別子: `acceptance:AC-ALN-001#acceptance_trace`
- 出典: `specs/requirements/align-review.fsl:230`
- 表題: 再推定は、固定した値を残し、固定していない値だけを書き換える

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `estimate_confident(0)`
  2. `human_edit(0)`
  3. `re_estimate(0)`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  value[0] == 2 and value[1] == 1 and pinned[0]
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-ALN-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 固定を解けば、次の再推定で自動の値に戻る

（出典: `specs/requirements/align-review.fsl:237`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-ALN-002#acceptance_trace" digest="sha256:c2d68812d374caf4a7fe710ada016e2b8c705ff07e10a2c2d5c61d985266039f" -->
#### 受け入れ基準: `AC-ALN-002`

- 識別子: `acceptance:AC-ALN-002#acceptance_trace`
- 出典: `specs/requirements/align-review.fsl:237`
- 表題: 固定を解けば、次の再推定で自動の値に戻る

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `estimate_confident(0)`
  2. `human_edit(0)`
  3. `revert_to_auto(0)`
  4. `re_estimate(0)`
- 期待（Then）: 最後の操作のあと、`value[0]` が `1` に等しい、かつ、`pinned[0]` が `false` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-ALN-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> まとめて確認は、確認待ちを一度にすべて確定させる

（出典: `specs/requirements/align-review.fsl:245`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-ALN-003#acceptance_trace" digest="sha256:5c45816a733f988a82bddd45e4ea4d847e7bce63093b9248f8aedbcff9f852f9" -->
#### 受け入れ基準: `AC-ALN-003`

- 識別子: `acceptance:AC-ALN-003#acceptance_trace`
- 出典: `specs/requirements/align-review.fsl:245`
- 表題: まとめて確認は、確認待ちを一度にすべて確定させる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `estimate_low_confidence(0)`
  2. `estimate_low_confidence(1)`
  3. `budget_exceeded_to_batch()`
  4. `confirm_all()`
- 期待（Then）: 最後の操作のあと、`entry[0]` が `AutoConfirmed` である、かつ、`entry[1]` が `AutoConfirmed` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-ALN-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 外部ツールで変わった値は、固定として取り込まれる

（出典: `specs/requirements/align-review.fsl:253`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-ALN-004#acceptance_trace" digest="sha256:48c47e7d1b9e77d6bc629039251f3a4842eb717eaff27659625d84dbf68b4409" -->
#### 受け入れ基準: `AC-ALN-004`

- 識別子: `acceptance:AC-ALN-004#acceptance_trace`
- 出典: `specs/requirements/align-review.fsl:253`
- 表題: 外部ツールで変わった値は、固定として取り込まれる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `estimate_confident(0)`
  2. `import_external_edit(0)`
  3. `re_estimate(0)`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  pinned[0] and value[0] == 2 and value[1] == 1
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認待ちが残ったまま書き出せない

（出典: `specs/requirements/align-review.fsl:215`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-001#forbidden_trace" digest="sha256:95324355cb9f92d58487abd660ebf3035d73bb812140a6dd18ac0691ebcfe4bc" -->
#### 禁止手順: `FB-ALN-001`

- 識別子: `forbidden:FB-ALN-001#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:215`
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

（出典: `specs/requirements/align-review.fsl:222`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-002#forbidden_trace" digest="sha256:b1fb3ca459fe5b417b842932a08831de4cea911504da8a743ca8e2c93067c226" -->
#### 禁止手順: `FB-ALN-002`

- 識別子: `forbidden:FB-ALN-002#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:222`
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

（出典: `specs/requirements/align-review.fsl:266`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-003#forbidden_trace" digest="sha256:97f1ec240dc37d4840a96da1a6b9133ff51b6fbdc53644bad36e6a9a7735f689" -->
#### 禁止手順: `FB-ALN-003`

- 識別子: `forbidden:FB-ALN-003#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:266`
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

### FB-ALN-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 個別確認モードのままでは、まとめて確認できない

（出典: `specs/requirements/align-review.fsl:260`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-ALN-004#forbidden_trace" digest="sha256:6d727207606efd33f6e28681f74e267408e7f3bb16e9a6006073d36219ab8741" -->
#### 禁止手順: `FB-ALN-004`

- 識別子: `forbidden:FB-ALN-004#forbidden_trace`
- 出典: `specs/requirements/align-review.fsl:260`
- 表題: 個別確認モードのままでは、まとめて確認できない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `estimate_low_confidence(0)`
- 期待（Then）: 続けて実行しようとする最後の操作 `confirm_all()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 自動の再推定は、固定された値に触れない

（出典: `specs/requirements/align-review.fsl:185`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:AutoPassNeverTouchesPinned#transition_rule" digest="sha256:68f8693bdeb4399d6affe6c29bcb883c270724593720e18b231e8c3ae5be7e5d" -->
#### 遷移条件: `AutoPassNeverTouchesPinned`

- 識別子: `property:trans:AutoPassNeverTouchesPinned#transition_rule`
- 出典: `specs/requirements/align-review.fsl:186`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
auto_pass => (forall s: Slot { old(pinned[s]) => value[s] == old(value[s]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 固定されている値は、人が入れた値である

（出典: `specs/requirements/align-review.fsl:180`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:PinnedMeansHumanValue#state_rule" digest="sha256:d70e301a2193c46dc288c866d6dd9a7c7c49c8877a1d30a3b141cd388589c4c4" -->
#### 状態不変条件: `PinnedMeansHumanValue`

- 識別子: `property:invariant:PinnedMeansHumanValue#state_rule`
- 出典: `specs/requirements/align-review.fsl:181`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `s: Slot` について、`pinned[s]` が `true` であるならば、`value[s]` が `2` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-ALN-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認が残っているものがあるうちは、書き出されていない

（出典: `specs/requirements/align-review.fsl:190`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoExportWhileReviewPending#state_rule" digest="sha256:684b7fe29a1167fd69420e33e6e0a7cb46e651bedef85b44ecf442522e95e98e" -->
#### 状態不変条件: `NoExportWhileReviewPending`

- 識別子: `property:invariant:NoExportWhileReviewPending#state_rule`
- 出典: `specs/requirements/align-review.fsl:191`

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

（出典: `specs/requirements/align-review.fsl:195`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ModeChangeNeedsBudget#state_rule" digest="sha256:c7bec772c8e591d3d990626f1dbf4a968447f6919f48ca95c7cf852f8279998c" -->
#### 状態不変条件: `ModeChangeNeedsBudget`

- 識別子: `property:invariant:ModeChangeNeedsBudget#state_rule`
- 出典: `specs/requirements/align-review.fsl:196`

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

（出典: `specs/requirements/align-review.fsl:57`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:estimate_confident#operation" digest="sha256:2fbd3654e93b26cbeed7decd553cb581bdc7420483895cbcfe4d105430d0e95b" -->
#### 操作: `estimate_confident`

- 識別子: `action:estimate_confident#operation`
- 出典: `specs/requirements/align-review.fsl:58`
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

（出典: `specs/requirements/align-review.fsl:65`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:estimate_low_confidence#operation" digest="sha256:61ac8735ed890ef4c25d3b1aa009be4d0675fbfbdc498948936e4eb2d163a490" -->
#### 操作: `estimate_low_confidence`

- 識別子: `action:estimate_low_confidence#operation`
- 出典: `specs/requirements/align-review.fsl:66`
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

（出典: `specs/requirements/align-review.fsl:73`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:text_deviation#operation" digest="sha256:331af99476aa83e7c67254e283cd3fae3515dc60559c6141db3d8c9fe71d59ba" -->
#### 操作: `text_deviation`

- 識別子: `action:text_deviation#operation`
- 出典: `specs/requirements/align-review.fsl:74`
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

（出典: `specs/requirements/align-review.fsl:210`）

> 書き出し前の検証で修復できない違反があれば、確認キューへ回して書き出しを止める

（出典: `specs/requirements/align-review.fsl:81`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:validation_unrepairable#operation" digest="sha256:5da0fed2e547076ba8139dbc38610222728c5855e7d6ae7d4af6f0b246ae16d5" -->
#### 操作: `validation_unrepairable`

- 識別子: `action:validation_unrepairable#operation`
- 出典: `specs/requirements/align-review.fsl:82`
- パラメータ: `e: Entry`

操作 `validation_unrepairable` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `AutoConfirmed` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `Blocked` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ExportBlocked#reachability_goal" digest="sha256:a1e22a49b4599ede25d2ddace772b898a5106e2453bbcde068b3ce8e60c3e89e" -->
#### 到達目標: `ExportBlocked`

- 識別子: `property:reachable:ExportBlocked#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:211`

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

（出典: `specs/requirements/align-review.fsl:89`）

> 外部ツールで変わった値は、次回読み込み時に固定として取り込む

（出典: `specs/requirements/align-review.fsl:106`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:human_edit#operation" digest="sha256:d67bd0555f2a35a61aa7d1f8433b2bac48645bef6571b05d68c5185c94e1ad28" -->
#### 操作: `human_edit`

- 識別子: `action:human_edit#operation`
- 出典: `specs/requirements/align-review.fsl:90`
- パラメータ: `s: Slot`

操作 `human_edit` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `exported` が `false` である。
2. `entry[s / VALUES]` が `NotEstimated` でない。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `true` にする。
2. `value[s]` を `2` にする。
3. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:import_external_edit#operation" digest="sha256:fe1e58118e3b2b7331b94306499ad832bdae50efac67cd0007c1abaaee6493a4" -->
#### 操作: `import_external_edit`

- 識別子: `action:import_external_edit#operation`
- 出典: `specs/requirements/align-review.fsl:107`
- パラメータ: `s: Slot`

操作 `import_external_edit` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `pinned[s]` が `false` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `true` にする。
2. `value[s]` を `2` にする。
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

（出典: `specs/requirements/align-review.fsl:98`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:revert_to_auto#operation" digest="sha256:cbeb3f3ef28060216349c34fac85ee82b229f81878514c784b1baa96e6e0f469" -->
#### 操作: `revert_to_auto`

- 識別子: `action:revert_to_auto#operation`
- 出典: `specs/requirements/align-review.fsl:99`
- パラメータ: `s: Slot`

操作 `revert_to_auto` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `pinned[s]` が `true` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pinned[s]` を `false` にする。
2. `auto_pass` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-ALN-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> 再推定しても、固定した値は残る

（出典: `specs/requirements/align-review.fsl:200`）

> 再推定は、固定されていない値だけを書き換える

（出典: `specs/requirements/align-review.fsl:115`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:re_estimate#operation" digest="sha256:59ffd47a3eaa42c67b9ec21cd27b8559dad3e3a3fd2c1f833f605788ded5b399" -->
#### 操作: `re_estimate`

- 識別子: `action:re_estimate#operation`
- 出典: `specs/requirements/align-review.fsl:116`
- パラメータ: `e: Entry`

操作 `re_estimate` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `NotEstimated` でない。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. すべての `s: Slot` について、次を適用する。

   1. `value[s]` を `if s / VALUES == e and not pinned[s] then 1 else value[s]` にする。
2. `auto_pass` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ReEstimateKeepsPinned#reachability_goal" digest="sha256:9c039b85f5ddb8c65ae62aaccd8d370ddf781b0528e89ace2865ab0c2f0528bd" -->
#### 到達目標: `ReEstimateKeepsPinned`

- 識別子: `property:reachable:ReEstimateKeepsPinned#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:201`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次のすべてが成立する。

1. `pinned[s]` が `true` である。

2. `value[s]` が `2` に等しい。

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

> 個別確認は、個別確認モードのときだけできる

（出典: `specs/requirements/align-review.fsl:125`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:confirm#operation" digest="sha256:9d2db0df480835dad8ef46416c8f3fb4caca06fe4831d91c92ba3393b1c041b1" -->
#### 操作: `confirm`

- 識別子: `action:confirm#operation`
- 出典: `specs/requirements/align-review.fsl:126`
- パラメータ: `e: Entry`

操作 `confirm` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `mode` が `Individual` である。
2. `entry[e]` が `InQueue` である。
3. `exported` が `false` である。

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

（出典: `specs/requirements/align-review.fsl:205`）

> 確認キューの項目は、oto の修正ではなく録り直しを選べる

（出典: `specs/requirements/align-review.fsl:144`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:rerecord#operation" digest="sha256:3cce77604048449992337b35400bd144d73bbd7355237be5e97a904b19dc5e24" -->
#### 操作: `rerecord`

- 識別子: `action:rerecord#operation`
- 出典: `specs/requirements/align-review.fsl:145`
- パラメータ: `e: Entry`

操作 `rerecord` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `entry[e]` が `InQueue` である、または、`entry[e]` が `Blocked` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `entry[e]` を `NotEstimated` にする。
2. `auto_pass` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RerecordFromQueue#reachability_goal" digest="sha256:fc37177fe415f2ab9eaf3ea6a78aa6ce0f521bee17cb4632675c551c84a233a0" -->
#### 到達目標: `RerecordFromQueue`

- 識別子: `property:reachable:RerecordFromQueue#reachability_goal`
- 出典: `specs/requirements/align-review.fsl:206`

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

> まとめて確認は、個別確認をやめたあとだけできる

（出典: `specs/requirements/align-review.fsl:134`）

> 上限を超えたとき、録り直し提案へ切り替えることもできる

（出典: `specs/requirements/align-review.fsl:162`）

> 確認の合計が上限を超えたら、個別確認をやめてまとめて確認か録り直し提案へ切り替える

（出典: `specs/requirements/align-review.fsl:153`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:budget_exceeded_to_batch#operation" digest="sha256:9b693ed26a729054873b376f818ff790a5fb8bba8ed36b1244238aa1da53a7f2" -->
#### 操作: `budget_exceeded_to_batch`

- 識別子: `action:budget_exceeded_to_batch#operation`
- 出典: `specs/requirements/align-review.fsl:154`
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

<!-- fsl:claim begin id="action:budget_exceeded_to_rerecord#operation" digest="sha256:1e70a7e5cb38588347bc1beea718ae3e5d251ef0e15c0a8d65952cd5c4f1e445" -->
#### 操作: `budget_exceeded_to_rerecord`

- 識別子: `action:budget_exceeded_to_rerecord#operation`
- 出典: `specs/requirements/align-review.fsl:163`
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

<!-- fsl:claim begin id="action:confirm_all#operation" digest="sha256:2a6ce51247a40ab1b422e2f3ad573b5455fa402fdb29ec3a5e285718bc130eb8" -->
#### 操作: `confirm_all`

- 識別子: `action:confirm_all#operation`
- 出典: `specs/requirements/align-review.fsl:135`
- パラメータ: なし

操作 `confirm_all` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `mode` が `Batch` である。
2. `exported` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. すべての `e: Entry` について、次を適用する。

   1. `entry[e]` を `if entry[e] == InQueue then AutoConfirmed else entry[e]` にする。
2. `auto_pass` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確認が残っている間は書き出せない

（出典: `specs/requirements/align-review.fsl:171`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export#operation" digest="sha256:33ed4d0fbc7600c2756cc1b712cb54351c1f7158cfb3f97c6ad7a9b1b261fe7c" -->
#### 操作: `export`

- 識別子: `action:export#operation`
- 出典: `specs/requirements/align-review.fsl:172`
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
- spec digest: `sha256:50a00022ad8e27a1fc46a4c63286a5018f85d1f3dea3e725d5e3d12d66159d55`
- claim set digest: `sha256:dcc1920e0b59fae1edeeac699cb69110093e94e2b0f0e755fe9cbaf8600ae6f6`
- 形式要素の分類: rendered 29 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 6 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 6 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
