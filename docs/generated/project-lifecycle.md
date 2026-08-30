---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/project-lifecycle.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:cf5269cc1d4d71475656fbe19753b757463bd2d581d0a76c9e8d86466be6d5e9
claim_set_digest: sha256:377438f4914ff31030ffcf6b7f6f6012c7d33652aec2517ca3f125cb6c1b561b
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

（出典: `specs/requirements/project-lifecycle.fsl:209`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-001#forbidden_trace" digest="sha256:609e13b0243fe4ca3eda6dca31de873715eae782fd30df9d1bf6db435061524c" -->
#### 禁止手順: `FB-PKG-001`

- 識別子: `forbidden:FB-PKG-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:209`
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

（出典: `specs/requirements/project-lifecycle.fsl:216`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-001#forbidden_trace" digest="sha256:8ec5dd3ef80b9fb971ff785f92090ae8d46561878fd131761178c0ab0619ae87" -->
#### 禁止手順: `FB-REC-001`

- 識別子: `forbidden:FB-REC-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:216`
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

（出典: `specs/requirements/project-lifecycle.fsl:140`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:CompletionIgnoresHandoff#state_rule" digest="sha256:5d5eb90f2a9f3618c80e59c8c8bd783cd6d158d80612faed46162dde26ba9ade" -->
#### 状態不変条件: `CompletionIgnoresHandoff`

- 識別子: `property:invariant:CompletionIgnoresHandoff#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:141`

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

（出典: `specs/requirements/project-lifecycle.fsl:165`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:ExportOnlyWhenComplete#transition_rule" digest="sha256:9c6d2b593a4a5abefe5d4edbf7304882a4564d5791076c169aa763711f785be5" -->
#### 遷移条件: `ExportOnlyWhenComplete`

- 識別子: `property:trans:ExportOnlyWhenComplete#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:166`

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

（出典: `specs/requirements/project-lifecycle.fsl:145`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:AdoptedImpliesTake#state_rule" digest="sha256:f87ec0f36f427b48e2e56cf0ff67b7086e6fe13d9f6d672adde1d57669f27633" -->
#### 状態不変条件: `AdoptedImpliesTake`

- 識別子: `property:invariant:AdoptedImpliesTake#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:146`

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

（出典: `specs/requirements/project-lifecycle.fsl:150`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:AllInvalidHasNoTake#state_rule" digest="sha256:64cb7e2bd36816cebdea82771eed1a629c5093c601729e3abe74511bc503b7b4" -->
#### 状態不変条件: `AllInvalidHasNoTake`

- 識別子: `property:invariant:AllInvalidHasNoTake#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:151`

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

（出典: `specs/requirements/project-lifecycle.fsl:155`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:UnrecordedHasNothing#state_rule" digest="sha256:67d7641147b7aa0abd2b577625d57074c35e78f79d7c432496de43f985669590" -->
#### 状態不変条件: `UnrecordedHasNothing`

- 識別子: `property:invariant:UnrecordedHasNothing#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:156`

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

（出典: `specs/requirements/project-lifecycle.fsl:160`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:TakesAreNeverLost#transition_rule" digest="sha256:144d7f94f1663f7b01d5d02b2371b6422f96931af6cf7688b48b255683461d51" -->
#### 遷移条件: `TakesAreNeverLost`

- 識別子: `property:trans:TakesAreNeverLost#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:161`

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

（出典: `specs/requirements/project-lifecycle.fsl:133`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingImpliesRoom#state_rule" digest="sha256:00a3bab269ea0e7292eaa3cc70185046700ecb85c5cc269521e1a40641188dc5" -->
#### 状態不変条件: `RecordingImpliesRoom`

- 識別子: `property:invariant:RecordingImpliesRoom#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:134`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`recording` が `some(i)` に等しいならば、（`takes[i]` が `MAX_TAKES` より小さい、かつ、`invalid[i]` が `MAX_TAKES` より小さい）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-REC-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-5: 復旧候補はクラッシュ1回につき最大1つ

（出典: `specs/requirements/project-lifecycle.fsl:185`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecoverableBounded#state_rule" digest="sha256:fbe9929ea3a0c67017eaf92b2f2cdf63135e577bb7c79cb5a9ff85bf55008366" -->
#### 状態不変条件: `RecoverableBounded`

- 識別子: `property:invariant:RecoverableBounded#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:186`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

各 `i: ListItem` にわたる `recoverable[i] + discarded[i]` の合計 が `crashes` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成した音源は配布 ZIP として書き出せる。書き出しは項目の状態を変えない

（出典: `specs/requirements/project-lifecycle.fsl:83`）

> 書き出しは項目の状態を変えない

（出典: `specs/requirements/project-lifecycle.fsl:170`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export_zip#operation" digest="sha256:3f0667023fb7f84695026d74b972b9338a9016691f3d344f3b6fbcea40d3fee9" -->
#### 操作: `export_zip`

- 識別子: `action:export_zip#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:84`
- パラメータ: なし

操作 `export_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. すべての `i: ListItem` について、`item[i]` が `Adopted` である。
2. `recording` が `none` に等しい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `handoff` を `Exported` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ExportDoesNotChangeItems#transition_rule" digest="sha256:c8cb9f61ed12ade69003c13613bb6a74ec01d2aac2d7d2f407de47c943b4ca48" -->
#### 遷移条件: `ExportDoesNotChangeItems`

- 識別子: `property:trans:ExportDoesNotChangeItems#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:171`

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

（出典: `specs/requirements/project-lifecycle.fsl:90`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:record_more_after_complete#operation" digest="sha256:d782c117d33e004ed300ded356e0cd9971e51bec5664670d4733714a06810f44" -->
#### 操作: `record_more_after_complete`

- 識別子: `action:record_more_after_complete#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:91`
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

（出典: `specs/requirements/project-lifecycle.fsl:52`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:4b8cce9c03a530844158373d042a25632dc19faad1ab48d1878127d7a0cb590c" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:53`
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

（出典: `specs/requirements/project-lifecycle.fsl:60`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finalize_valid_take#operation" digest="sha256:005bb9569508496d09186e528276dfe00f05357f44e7cf803a2f1c671e26eaa7" -->
#### 操作: `finalize_valid_take`

- 識別子: `action:finalize_valid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:61`
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

（出典: `specs/requirements/project-lifecycle.fsl:204`）

> 取りこぼしを検出したテイクは無効として保存し、採用テイクにしない

（出典: `specs/requirements/project-lifecycle.fsl:68`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:discard_invalid_take#operation" digest="sha256:e8eac3765ef84ab5b00789eca401faab4daa874ecd0d3e2002d305ae496ef9bb" -->
#### 操作: `discard_invalid_take`

- 識別子: `action:discard_invalid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:69`
- パラメータ: なし

操作 `discard_invalid_take` を実行できるのは、次の条件を満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `invalid[i]` を `invalid[i] + 1` にする。
3. `item[i]` を `if takes[i] > 0 then Adopted else AllInvalid` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ItemCanBeAllInvalid#reachability_goal" digest="sha256:535c605dfe0e7cf69cdee69423227e823033a91f8a2b85f963c0b6396db14945" -->
#### 到達目標: `ItemCanBeAllInvalid`

- 識別子: `property:reachable:ItemCanBeAllInvalid#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:205`

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

（出典: `specs/requirements/project-lifecycle.fsl:175`）

> 異常終了で失われるのは進行中のテイクだけで、確定済みは残る

（出典: `specs/requirements/project-lifecycle.fsl:98`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:crash_losing_take#operation" digest="sha256:d7d843deb560fb9cc6cb95049bbe872d663c8d6cb16f379760791d394baa53f5" -->
#### 操作: `crash_losing_take`

- 識別子: `action:crash_losing_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:99`
- パラメータ: なし

操作 `crash_losing_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しくない。
2. `crashes` が `MAX_CRASHES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:RecoverableNeverVanishesOnItsOwn#transition_rule" digest="sha256:9e38480f1d40c94e96e84900eb5a25de2fa7e386683ede285fd383e0b62348bc" -->
#### 遷移条件: `RecoverableNeverVanishesOnItsOwn`

- 識別子: `property:trans:RecoverableNeverVanishesOnItsOwn#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:177`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の少なくとも一つが成立する。

1. `recoverable[i]` が 遷移前の `recoverable[i]` 以上である。

2. `takes[i]` が 遷移前の `takes[i]` より大きい。

3. `discarded[i]` が 遷移前の `discarded[i]` より大きい。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録り直しても過去のテイクは残り、採用をいつでも戻せる

（出典: `specs/requirements/project-lifecycle.fsl:76`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:readopt_earlier_take#operation" digest="sha256:599aa09459e14d5e734ab41218d887007b677ea7fd20c4da6b9baca9724e37b8" -->
#### 操作: `readopt_earlier_take`

- 識別子: `action:readopt_earlier_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:77`
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

### REQ-REC-006

**要件原文（意図。形式意味との一致は人間が確認する）**

> 確定まで進んでいたテイクは、クラッシュしても復旧候補として残る

（出典: `specs/requirements/project-lifecycle.fsl:106`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:crash_leaving_recoverable#operation" digest="sha256:f59db24399fabedc23d9e7d5c0eca3346220defb6372c7df5c19fe95273f68f7" -->
#### 操作: `crash_leaving_recoverable`

- 識別子: `action:crash_leaving_recoverable#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:107`
- パラメータ: なし

操作 `crash_leaving_recoverable` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。
2. `crashes` が `MAX_CRASHES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recoverable[i]` を `recoverable[i] + 1` にする。
2. `recording` を `none` にする。
3. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> 復旧候補が消えるのは、本人が採ったか捨てたときだけ

（出典: `specs/requirements/project-lifecycle.fsl:176`）

> 復旧候補は本人が捨てることができる

（出典: `specs/requirements/project-lifecycle.fsl:125`）

> 復旧候補は本人が採ることができる

（出典: `specs/requirements/project-lifecycle.fsl:115`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:discard_recoverable#operation" digest="sha256:b25c286f8dabb8616579c8af9e55b5690099e5e2e266feec95558c0f6a1c873e" -->
#### 操作: `discard_recoverable`

- 識別子: `action:discard_recoverable#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:126`
- パラメータ: `i: ListItem`

操作 `discard_recoverable` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `recoverable[i]` が `0` より大きい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recoverable[i]` を `recoverable[i] - 1` にする。
2. `discarded[i]` を `discarded[i] + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:recover_take#operation" digest="sha256:adb853a82a396d791ac7308b2424f3bf74ed57a0243e4f680ac235d3b161cac9" -->
#### 操作: `recover_take`

- 識別子: `action:recover_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:116`
- パラメータ: `i: ListItem`

操作 `recover_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `recoverable[i]` が `0` より大きい。
3. `takes[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recoverable[i]` を `recoverable[i] - 1` にする。
2. `takes[i]` を `takes[i] + 1` にする。
3. `item[i]` を `Adopted` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

この遷移条件の内容は、`REQ-REC-004` の節に記載している。この要件にも同じ意味で適用される。

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> すべての項目が揃う前でも、採用テイクを持つ項目が現れる

（出典: `specs/requirements/project-lifecycle.fsl:199`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:PartiallyRecorded#reachability_goal" digest="sha256:ba77fb3dd0ebf413d063c42eaa6c91c7bb744870570b77680a7d2d1b752b41e1" -->
#### 到達目標: `PartiallyRecorded`

- 識別子: `property:reachable:PartiallyRecorded#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:200`

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

（出典: `specs/requirements/project-lifecycle.fsl:194`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:CompleteWhileUnpublished#reachability_goal" digest="sha256:db03f3439bc79b67c43f06c538cf9a9ff587f1a4995fed728105da74a8404473" -->
#### 到達目標: `CompleteWhileUnpublished`

- 識別子: `property:reachable:CompleteWhileUnpublished#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:195`

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

<!-- fsl:claim begin id="property:trans:CrashKeepsCommittedTakes#transition_rule" digest="sha256:b578ba21983b62e09a62002104be0d4c0223dbb977ccf9e99a4e85b023f1e8c4" -->
#### 遷移条件: `CrashKeepsCommittedTakes`

- 識別子: `property:trans:CrashKeepsCommittedTakes#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:190`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
crashes != old(crashes) => (forall i: ListItem { takes[i] == old(takes[i]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

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
- spec digest: `sha256:cf5269cc1d4d71475656fbe19753b757463bd2d581d0a76c9e8d86466be6d5e9`
- claim set digest: `sha256:377438f4914ff31030ffcf6b7f6f6012c7d33652aec2517ca3f125cb6c1b561b`
- 形式要素の分類: rendered 25 件 / unattributed 2 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 7 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 7 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
