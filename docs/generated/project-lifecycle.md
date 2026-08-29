---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/project-lifecycle.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:dc34791a6ac4291bfe112225183923ebdd03d3a69337425083621475a48d56f0
claim_set_digest: sha256:992fabd3853d262f732c78a64c052af1225ae4e6b00dfc0e39e90b8d58619876
---

# 要件仕様書: KoeruProjectLifecycle

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

### FB-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成していない音源は書き出せない

（出典: `specs/requirements/project-lifecycle.fsl:161`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-001#forbidden_trace" digest="sha256:e1bf983bb862481189a69bd5718973c4cccf63027f1cb1e01f1e2c174efbfaa8" -->
#### 禁止手順: `FB-PKG-001`

- 識別子: `forbidden:FB-PKG-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:161`
- 表題: 完成していない音源は書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `start_take(0)`
  2. `finalize_valid_take()`
- 期待（Then）: 続けて実行しようとする最後の操作 `export_zip()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 無効テイクだけでは完成に到達しない

（出典: `specs/requirements/project-lifecycle.fsl:168`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-001#forbidden_trace" digest="sha256:53624224dc600322afb3503517d35d2cafa0257539e7181cb727d73ddef449c8" -->
#### 禁止手順: `FB-REC-001`

- 識別子: `forbidden:FB-REC-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:168`
- 表題: 無効テイクだけでは完成に到達しない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `start_take(0)`
  2. `discard_invalid_take()`
  3. `start_take(1)`
  4. `discard_invalid_take()`
- 期待（Then）: 続けて実行しようとする最後の操作 `export_zip()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成は項目の状態だけで決まり、書き出し履歴を一切参照しない

（出典: `specs/requirements/project-lifecycle.fsl:106`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:CompletionIgnoresHandoff#state_rule" digest="sha256:f291eac894151f612ac6a9a7563587f1bfd17fc86e4ff6a6e85c012b613afdb3" -->
#### 状態不変条件: `CompletionIgnoresHandoff`

- 識別子: `property:invariant:CompletionIgnoresHandoff#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:107`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall i: ListItem { item[i] == Adopted }) => (forall i: ListItem { item[i] == Adopted })
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 書き出せるのは完成しているときだけ

（出典: `specs/requirements/project-lifecycle.fsl:131`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:ExportOnlyWhenComplete#transition_rule" digest="sha256:3a18f91266580c443b41c47f1380c19861dfe89268d51efcda1aa9045991474c" -->
#### 遷移条件: `ExportOnlyWhenComplete`

- 識別子: `property:trans:ExportOnlyWhenComplete#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:132`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
handoff != old(handoff) => (forall i: ListItem { item[i] == Adopted })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 採用テイクを持つ項目は、確定したテイクを1件以上持つ

（出典: `specs/requirements/project-lifecycle.fsl:111`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:AdoptedImpliesTake#state_rule" digest="sha256:bbf26cfe4a924f19b3e69051d697504507e448c404a486c27d1607212c03ec4a" -->
#### 状態不変条件: `AdoptedImpliesTake`

- 識別子: `property:invariant:AdoptedImpliesTake#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:112`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `Adopted` であるならば、`takes[i]` が `0` より大きい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 全テイク無効の項目は、確定したテイクを持たない

（出典: `specs/requirements/project-lifecycle.fsl:116`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:AllInvalidHasNoTake#state_rule" digest="sha256:918dd985ed10affaaeeeb2d6a28273da0288d2dc3087ce03415c8dedaf2fc59e" -->
#### 状態不変条件: `AllInvalidHasNoTake`

- 識別子: `property:invariant:AllInvalidHasNoTake#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:117`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `AllInvalid` であるならば、`takes[i]` が `0` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 未収録の項目は、確定したテイクも無効テイクも持たない

（出典: `specs/requirements/project-lifecycle.fsl:121`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:UnrecordedHasNothing#state_rule" digest="sha256:584ba2d837eee0f65a1cf9e728b731dfbf65a3b75e019280ee89ff4a73ceb92a" -->
#### 状態不変条件: `UnrecordedHasNothing`

- 識別子: `property:invariant:UnrecordedHasNothing#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:122`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `Unrecorded` であるならば、（`takes[i]` が `0` に等しい、かつ、`invalid[i]` が `0` に等しい）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確定したテイクは、削除も上書きもされない

（出典: `specs/requirements/project-lifecycle.fsl:126`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:TakesAreNeverLost#transition_rule" digest="sha256:95d8b76669efc51a963337ad6843c225381e0e8ab40f6d049112fa702ea1d4e7" -->
#### 遷移条件: `TakesAreNeverLost`

- 識別子: `property:trans:TakesAreNeverLost#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:127`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

すべての `i: ListItem` について、`takes[i]` が 遷移前の `takes[i]` 以上である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-REC-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-3: 収録中の項目は、まだ録り足せる余地がある

（出典: `specs/requirements/project-lifecycle.fsl:99`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingImpliesRoom#state_rule" digest="sha256:fcc42fcdea88c5cc05779c7bc66d4f18c11f1f0904b27028994dd3ac8fa22a18" -->
#### 状態不変条件: `RecordingImpliesRoom`

- 識別子: `property:invariant:RecordingImpliesRoom#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:100`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`recording` が `some(i)` に等しいならば、（`takes[i]` が `MAX_TAKES` より小さい、かつ、`invalid[i]` が `MAX_TAKES` より小さい）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成した音源は配布 ZIP として書き出せる。書き出しは項目の状態を変えない

（出典: `specs/requirements/project-lifecycle.fsl:76`）

> 書き出しは項目の状態を変えない

（出典: `specs/requirements/project-lifecycle.fsl:136`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export_zip#operation" digest="sha256:3c4ed46d242eaecf06cfb02e8a359b1f2509198f273061ae712cce2ba73df5f5" -->
#### 操作: `export_zip`

- 識別子: `action:export_zip#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:77`
- パラメータ: なし

操作 `export_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. すべての `i: ListItem` について、`item[i]` が `Adopted` である。
2. `recording` が `none` に等しい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `handoff` を `Exported` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ExportDoesNotChangeItems#transition_rule" digest="sha256:ab61fcb233631824742cfd977f7209ba83e1d64f4bcde8f8f5254abbe8dd8322" -->
#### 遷移条件: `ExportDoesNotChangeItems`

- 識別子: `property:trans:ExportDoesNotChangeItems#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:137`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
handoff != old(handoff) => (forall i: ListItem { item[i] == old(item[i]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成後も録り足せる。無効テイクを積んでも採用は外れない

（出典: `specs/requirements/project-lifecycle.fsl:83`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:record_more_after_complete#operation" digest="sha256:f58500f4dac34497003d1a32b61287fcffe5b519a99d2493a28984dee450959c" -->
#### 操作: `record_more_after_complete`

- 識別子: `action:record_more_after_complete#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:84`
- パラメータ: `i: ListItem`

操作 `record_more_after_complete` を実行できるのは、次の条件をすべて満たす場合に限る。

1. すべての `i: ListItem` について、`item[i]` が `Adopted` である。
2. `recording` が `none` に等しい。
3. `invalid[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `invalid[i]` を `invalid[i] + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録音を開始すると、確定するまで進行中の項目が1つある

（出典: `specs/requirements/project-lifecycle.fsl:45`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:048152a1c70061de693fa4704ebe9d437656c09b378114080342868038cf7440" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:46`
- パラメータ: `i: ListItem`

操作 `start_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `takes[i]` が `MAX_TAKES` より小さい。
3. `invalid[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `some(i)` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 取りこぼしのないテイクは世代として積まれ、その項目の採用テイクになる

（出典: `specs/requirements/project-lifecycle.fsl:53`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finalize_valid_take#operation" digest="sha256:39bb4f1f794a1ff8af9a3c12a1d6c6dec486e91a98396f71f874563312232c5b" -->
#### 操作: `finalize_valid_take`

- 識別子: `action:finalize_valid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:54`
- パラメータ: なし

操作 `finalize_valid_take` を実行できるのは、次の条件を満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `takes[i]` を `takes[i] + 1` にする。
3. `item[i]` を `Adopted` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 全テイク無効の項目が生じうる

（出典: `specs/requirements/project-lifecycle.fsl:156`）

> 取りこぼしを検出したテイクは無効として保存し、採用テイクにしない

（出典: `specs/requirements/project-lifecycle.fsl:61`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:discard_invalid_take#operation" digest="sha256:f5027678f2b66c4fea03dca595f3334cb1667f658f04473562c9c0ef612c64b9" -->
#### 操作: `discard_invalid_take`

- 識別子: `action:discard_invalid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:62`
- パラメータ: なし

操作 `discard_invalid_take` を実行できるのは、次の条件を満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `invalid[i]` を `invalid[i] + 1` にする。
3. `item[i]` を `if takes[i] > 0 then Adopted else AllInvalid` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ItemCanBeAllInvalid#reachability_goal" digest="sha256:66b1449a7cf0704b6b807bde01ac9c0c4774865db99783c821e2dd5846c80dc2" -->
#### 到達目標: `ItemCanBeAllInvalid`

- 識別子: `property:reachable:ItemCanBeAllInvalid#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:157`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `i: ListItem` が存在して、`item[i]` が `AllInvalid` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 異常終了でも、確定したテイクの数は減らない

（出典: `specs/requirements/project-lifecycle.fsl:141`）

> 異常終了で失われるのは進行中のテイクだけで、確定済みは残る

（出典: `specs/requirements/project-lifecycle.fsl:91`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:crash_and_recover#operation" digest="sha256:9df0a41ce16400843d4969c9197c84cc1f134de6196eaa8bc7fe4689a5568d75" -->
#### 操作: `crash_and_recover`

- 識別子: `action:crash_and_recover#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:92`
- パラメータ: なし

操作 `crash_and_recover` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しくない。
2. `crashes` が `MAX_CRASHES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:CrashKeepsCommittedTakes#transition_rule" digest="sha256:6106d5d1b82e46245e3148ef0aca111df2d66104aa2823d7270d8d7c307d7c81" -->
#### 遷移条件: `CrashKeepsCommittedTakes`

- 識別子: `property:trans:CrashKeepsCommittedTakes#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:142`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
crashes != old(crashes) => (forall i: ListItem { takes[i] == old(takes[i]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録り直しても過去のテイクは残り、採用をいつでも戻せる

（出典: `specs/requirements/project-lifecycle.fsl:69`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:readopt_earlier_take#operation" digest="sha256:b9b6515e4bee82692b046d0e3a82732aa6a54f011ff922f4b18c7066f8d77f93" -->
#### 操作: `readopt_earlier_take`

- 識別子: `action:readopt_earlier_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:70`
- パラメータ: `i: ListItem`

操作 `readopt_earlier_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `takes[i]` が `2` 以上である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `item[i]` を `Adopted` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> すべての項目が揃う前でも、採用テイクを持つ項目が現れる

（出典: `specs/requirements/project-lifecycle.fsl:151`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:PartiallyRecorded#reachability_goal" digest="sha256:9ace2877909b8cb90f2eb908c6ce673d7a3c7c76e50166a264d28d30c699a995" -->
#### 到達目標: `PartiallyRecorded`

- 識別子: `property:reachable:PartiallyRecorded#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:152`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(exists i: ListItem { item[i] == Adopted }) and not (forall i: ListItem { item[i] == Adopted })
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-VIS-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 公開操作なしに完成状態へ到達できる

（出典: `specs/requirements/project-lifecycle.fsl:146`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:CompleteWhileUnpublished#reachability_goal" digest="sha256:118df0063bb61a623a5e37969852142c5253d91a8a5ee430a2647906e6756c45" -->
#### 到達目標: `CompleteWhileUnpublished`

- 識別子: `property:reachable:CompleteWhileUnpublished#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:147`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall i: ListItem { item[i] == Adopted }) and handoff == NotExported
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

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:1f1e05f23be0afef33318e7c66dda338600861b3024bdc377127e32707969bfd" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
recording == none and ((forall i: ListItem { item[i] == Adopted }) and handoff == Exported or (forall i: ListItem { takes[i] >= MAX_TAKES or invalid[i] >= MAX_TAKES }))
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `ListItem` の解析インスタンス数: 2
- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/project-lifecycle.fsl`（`KoeruProjectLifecycle`、dialect: `requirements`）
- spec digest: `sha256:dc34791a6ac4291bfe112225183923ebdd03d3a69337425083621475a48d56f0`
- claim set digest: `sha256:992fabd3853d262f732c78a64c052af1225ae4e6b00dfc0e39e90b8d58619876`
- 形式要素の分類: rendered 21 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 7 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 7 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
