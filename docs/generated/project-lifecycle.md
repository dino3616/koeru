---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/project-lifecycle.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:52d881afba64195c55965343201e952f7da0fd91f05973540fdc8d59f70f08af
claim_set_digest: sha256:ded9671414d726296c3514272a6a058c420ab3e8037be0a61a41535b313f01c5
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

### AC-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成したあとに書き出しても完成状態は変わらない

（出典: `specs/requirements/project-lifecycle.fsl:134`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PKG-001#acceptance_trace" digest="sha256:f00d40ceb5fdf6991f97181b600c2a553310760b4c0f6283f04a1f6df111a800" -->
#### 受け入れ基準: `AC-PKG-001`

- 識別子: `acceptance:AC-PKG-001#acceptance_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:134`
- 表題: 完成したあとに書き出しても完成状態は変わらない

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `start_take()`
  2. `finalize_valid_take()`
  3. `start_take()`
  4. `finalize_valid_take()`
  5. `start_take()`
  6. `finalize_valid_take()`
  7. `export_zip()`
- 期待（Then）: 最後の操作のあと、`coverage` が `Complete` である、かつ、`handoff` が `Exported` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-VIS-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 公開操作を一度も行わずに完成へ到達する

（出典: `specs/requirements/project-lifecycle.fsl:127`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-VIS-001#acceptance_trace" digest="sha256:f1c1a416a8a73ed0e8231b37e4f6f0b164c717773bfd8206189887b81a0ff90d" -->
#### 受け入れ基準: `AC-VIS-001`

- 識別子: `acceptance:AC-VIS-001#acceptance_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:127`
- 表題: 公開操作を一度も行わずに完成へ到達する

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `start_take()`
  2. `finalize_valid_take()`
  3. `start_take()`
  4. `finalize_valid_take()`
  5. `start_take()`
  6. `finalize_valid_take()`
- 期待（Then）: 最後の操作のあと、`coverage` が `Complete` である、かつ、`handoff` が `NotExported` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成していない音源は書き出せない

（出典: `specs/requirements/project-lifecycle.fsl:142`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-001#forbidden_trace" digest="sha256:38a56b6c758ca2ad5b3212d5179f409a3024fd9b3790a8191b69593a3d6bb08f" -->
#### 禁止手順: `FB-PKG-001`

- 識別子: `forbidden:FB-PKG-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:142`
- 表題: 完成していない音源は書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `start_take()`
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

（出典: `specs/requirements/project-lifecycle.fsl:149`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-001#forbidden_trace" digest="sha256:a7976e0c5ec33316c49e6dd6ca100ef6cc4c7647351fd9e42bd4854e41337d25" -->
#### 禁止手順: `FB-REC-001`

- 識別子: `forbidden:FB-REC-001#forbidden_trace`
- 出典: `specs/requirements/project-lifecycle.fsl:149`
- 表題: 無効テイクだけでは完成に到達しない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `start_take()`
  2. `discard_invalid_take()`
  3. `start_take()`
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

> 完成判定は coverage のみで決まり、書き出し履歴を一切参照しない

（出典: `specs/requirements/project-lifecycle.fsl:92`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:CoverageIsMechanical#state_rule" digest="sha256:709dea6c2f6ca734741385ac325f5a8d62580f26893123b20a3fb7ffa52c4493" -->
#### 状態不変条件: `CoverageIsMechanical`

- 識別子: `property:invariant:CoverageIsMechanical#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:93`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`coverage == Complete` が `valid_takes >= REQUIRED_TAKES` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 書き出せるのは完成しているときだけ

（出典: `specs/requirements/project-lifecycle.fsl:107`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:ExportOnlyWhenComplete#transition_rule" digest="sha256:112930d9f68ed1c37f8d85b33954ca13288e40901eea9cc994d4978df3b6fcdf" -->
#### 遷移条件: `ExportOnlyWhenComplete`

- 識別子: `property:trans:ExportOnlyWhenComplete#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:108`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`handoff` が 遷移前の `handoff` に等しくないならば、`coverage` が `Complete` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 無効テイクはカバレッジに加算されない

（出典: `specs/requirements/project-lifecycle.fsl:97`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:InvalidTakesNeverCount#state_rule" digest="sha256:b58e56d6ff7ae4885701fa3a1408c0a1b8500f3bd577ae9261959b233f92117d" -->
#### 状態不変条件: `InvalidTakesNeverCount`

- 識別子: `property:invariant:InvalidTakesNeverCount#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:98`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`coverage` が `Complete` であるならば、`valid_takes` が `REQUIRED_TAKES` 以上である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-4: 確定テイク数は検証用に必須本数を上限とする

（出典: `specs/requirements/project-lifecycle.fsl:89`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:TakesBounded#state_rule" digest="sha256:f5954e731c3b92a9cf5c38c6080cd3a19dff2896fa7b75c8692710454ed5b846" -->
#### 状態不変条件: `TakesBounded`

- 識別子: `property:invariant:TakesBounded#state_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:90`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`valid_takes` が `REQUIRED_TAKES` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> ZIP の書き出しは完成状態を変更しない

（出典: `specs/requirements/project-lifecycle.fsl:102`）

> 完成した音源は配布 ZIP として書き出せる。書き出しは完成状態を変えない

（出典: `specs/requirements/project-lifecycle.fsl:66`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:export_zip#operation" digest="sha256:86a5d6b71dbfa6631dece1d39f7c29a7f76f802f6de196b0210944e785a0ad92" -->
#### 操作: `export_zip`

- 識別子: `action:export_zip#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:67`
- パラメータ: なし

操作 `export_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `coverage` が `Complete` である。
2. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `handoff` を `Exported` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ExportDoesNotChangeCoverage#transition_rule" digest="sha256:ce7af8063b173decea32ec906f7ce9c58bdd5146d5a0a3d99b279bedfe580646" -->
#### 遷移条件: `ExportDoesNotChangeCoverage`

- 識別子: `property:trans:ExportDoesNotChangeCoverage#transition_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:103`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`handoff` が 遷移前の `handoff` に等しくないならば、`coverage` が 遷移前の `coverage` に等しい。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 完成後も編集でき、被覆が崩れれば完成状態から外れる

（出典: `specs/requirements/project-lifecycle.fsl:73`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:edit_breaks_coverage#operation" digest="sha256:2032b0652c564f35abc597a1c0f7ab2d6c384d73105047ed6e572dfed92aa2e1" -->
#### 操作: `edit_breaks_coverage`

- 識別子: `action:edit_breaks_coverage#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:74`
- パラメータ: なし

操作 `edit_breaks_coverage` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `coverage` が `Complete` である。
2. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `valid_takes` を `valid_takes - 1` にする。
2. `coverage` を `Partial` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録音を開始すると、確定するまで進行中のテイクが1件ある

（出典: `specs/requirements/project-lifecycle.fsl:43`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:a1b200f121f592a4bdc5ee4a5c428658d3fa0167be78e433c20c11ee3e3e3b82" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:44`
- パラメータ: なし

操作 `start_take` を実行できるのは、次の条件を満たす場合に限る。

1. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 取りこぼしのないテイクは確定し、カバレッジに加算される

（出典: `specs/requirements/project-lifecycle.fsl:49`）

> 録り続ければいずれ完成へ到達する

（出典: `specs/requirements/project-lifecycle.fsl:112`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finalize_valid_take#operation" digest="sha256:4abe120ccdb367944652d7e284459852a2ec166e6d610aecceab4cbb57d473dd" -->
#### 操作: `finalize_valid_take`

- 識別子: `action:finalize_valid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:50`
- パラメータ: なし

操作 `finalize_valid_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `true` である。
2. `valid_takes` が `REQUIRED_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `false` にする。
2. `valid_takes` を `valid_takes + 1` にする。
3. `coverage` を `if valid_takes + 1 >= REQUIRED_TAKES then Complete else Partial` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:leadsTo:EventuallyComplete#progress_rule" digest="sha256:b88fa8e3c759e6b258d88e1f04654829fecc738cde60141eeca38b96882ffc08" -->
#### 進行条件: `EventuallyComplete`

- 識別子: `property:leadsTo:EventuallyComplete#progress_rule`
- 出典: `specs/requirements/project-lifecycle.fsl:113`

次の進行条件（liveness）が、FSL 上の要求として宣言されている。

- 起点: 「`coverage` が `Partial` である」が成立したとき。
- 帰結: それ以降のいつかの時点で、「`coverage` が `Complete` である」が成立しなければならない。期限（within）は指定されていない。
- 前提: この進行条件の成立は、各操作に宣言された公平性の仮定（弱い公平性）に依存し得る。公平性の宣言は各操作の記述を参照。
- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。この条件は FSL が要求として宣言しているものであり、成立が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 取りこぼし（xrun）を検出したテイクは無効として保存し、カバレッジに加算しない

（出典: `specs/requirements/project-lifecycle.fsl:58`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:discard_invalid_take#operation" digest="sha256:1d2d55e4e98eaf705e87298f26a054a4f3dbdc4de998b85e678dfe1dc4098df9" -->
#### 操作: `discard_invalid_take`

- 識別子: `action:discard_invalid_take#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:59`
- パラメータ: なし

操作 `discard_invalid_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `true` である。
2. `invalid_takes` が `2` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `false` にする。
2. `invalid_takes` を `invalid_takes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 異常終了で失われるのは進行中のテイクだけで、確定済みは残る

（出典: `specs/requirements/project-lifecycle.fsl:81`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:crash_and_recover#operation" digest="sha256:5c9ce00cae11d19106c0da4a60e554061c2ecb5208564ed83ca61c3d88225b22" -->
#### 操作: `crash_and_recover`

- 識別子: `action:crash_and_recover#operation`
- 出典: `specs/requirements/project-lifecycle.fsl:82`
- パラメータ: なし

操作 `crash_and_recover` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `true` である。
2. `crashes` が `2` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `false` にする。
2. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 必要な本数が揃う前でも試唱できる

（出典: `specs/requirements/project-lifecycle.fsl:122`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:PreviewableBeforeComplete#reachability_goal" digest="sha256:de262cc12725a48b5acbf94ee2544f40fcff4ff48eb174d98c366a3f8a3eb3f9" -->
#### 到達目標: `PreviewableBeforeComplete`

- 識別子: `property:reachable:PreviewableBeforeComplete#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:123`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`valid_takes` が `PREVIEW_MIN` 以上である、かつ、`coverage` が `Complete` でない。

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

（出典: `specs/requirements/project-lifecycle.fsl:117`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:CompleteWhileUnpublished#reachability_goal" digest="sha256:cdb70eb4924165c06ef63d824a1417700f5da01f6c01820919cbdddcc92e7196" -->
#### 到達目標: `CompleteWhileUnpublished`

- 識別子: `property:reachable:CompleteWhileUnpublished#reachability_goal`
- 出典: `specs/requirements/project-lifecycle.fsl:118`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`coverage` が `Complete` である、かつ、`handoff` が `NotExported` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:6f1c150be6eed609ec9523aecad0c42fe45f74e064e633704f6ee08093267625" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次のすべてが成立する。

1. `coverage` が `Complete` である。

2. `handoff` が `Exported` である。

3. `recording` が `false` である。

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- 数値 `TakeCount` の解析値域: `0` から `4` まで

## 生成情報

- 生成元仕様: `specs/requirements/project-lifecycle.fsl`（`KoeruProjectLifecycle`、dialect: `requirements`）
- spec digest: `sha256:52d881afba64195c55965343201e952f7da0fd91f05973540fdc8d59f70f08af`
- claim set digest: `sha256:ded9671414d726296c3514272a6a058c420ab3e8037be0a61a41535b313f01c5`
- 形式要素の分類: rendered 18 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 0 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
