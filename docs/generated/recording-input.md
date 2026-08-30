---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/recording-input.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:a5d5a1438298543f27c6dc08719bbedcc639d94388a9c51555a7c9d58ef83946
claim_set_digest: sha256:edb9464e528fd4bc9e54fe8deeb350fbb82e42a9a82f139492dc3e37ae1a2578
---

# 要件仕様書: KoeruRecordingInput

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

### AC-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 選んで、開いて、効果を無効化し、校正し、生死と残量を確かめて録る

（出典: `specs/requirements/recording-input.fsl:305`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-REC-101#acceptance_trace" digest="sha256:b522684e58436942901d5a10b1258e0722a185ca9778c8ac733efbc7c4e97a53" -->
#### 受け入れ基準: `AC-REC-101`

- 識別子: `acceptance:AC-REC-101#acceptance_trace`
- 出典: `specs/requirements/recording-input.fsl:305`
- 表題: 選んで、開いて、効果を無効化し、校正し、生死と残量を確かめて録る

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `estimate_space_enough()`
  7. `start_take()`
  8. `finish_take()`
- 期待（Then）: 最後の操作のあと、`takes` が `1` に等しい、かつ、`stream_open` が `true` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> デバイスが消失したまま収録を始められない

（出典: `specs/requirements/recording-input.fsl:317`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-101#forbidden_trace" digest="sha256:d25197560b3542a7b4fa6eb0ba0355f5c278dd5b2b942cf59416fb3e08c0f592" -->
#### 禁止手順: `FB-REC-101`

- 識別子: `forbidden:FB-REC-101#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:317`
- 表題: デバイスが消失したまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `estimate_space_enough()`
  7. `device_lost()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_take()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 入力が届いていないまま収録を始められない

（出典: `specs/requirements/recording-input.fsl:329`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-102#forbidden_trace" digest="sha256:59ac2c7d6c309ba809a702709f34bf0dfb7735d16af785dfe35a2527e65bf161" -->
#### 禁止手順: `FB-REC-102`

- 識別子: `forbidden:FB-REC-102#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:329`
- 表題: 入力が届いていないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_dead()`
  6. `estimate_space_enough()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_take()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 校正しないまま収録を始められない

（出典: `specs/requirements/recording-input.fsl:340`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-103#forbidden_trace" digest="sha256:b18f2b5e167177422c087b7f398ee0805f841340e9cb59a4b581b68fb42065fa" -->
#### 禁止手順: `FB-REC-103`

- 識別子: `forbidden:FB-REC-103#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:340`
- 表題: 校正しないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `input_is_alive()`
  5. `estimate_space_enough()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_take()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録中に手順の提示を出せない

（出典: `specs/requirements/recording-input.fsl:350`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-104#forbidden_trace" digest="sha256:4bfce2a8e0d6051080158c2efd85ee24e4c8b8932027b0f1b099af7291211eda" -->
#### 禁止手順: `FB-REC-104`

- 識別子: `forbidden:FB-REC-104#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:350`
- 表題: 収録中に手順の提示を出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `some_effects_remain()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `estimate_space_enough()`
  7. `start_take()`
- 期待（Then）: 続けて実行しようとする最後の操作 `show_prompt_once()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 回り込みを確認しないままガイドを鳴らせない

（出典: `specs/requirements/recording-input.fsl:383`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-105#forbidden_trace" digest="sha256:e02f3c6e0addec6ba83626b520cbce455c7468d5c9725626f593f6bdfe7a7bed" -->
#### 禁止手順: `FB-REC-105`

- 識別子: `forbidden:FB-REC-105#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:383`
- 表題: 回り込みを確認しないままガイドを鳴らせない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
- 期待（Then）: 続けて実行しようとする最後の操作 `enable_guide()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> 残量が足りないまま収録を始められない

（出典: `specs/requirements/recording-input.fsl:362`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-106#forbidden_trace" digest="sha256:53bfb6ae4455a7c5cfca551b41ac097f4bce6f07904d6ea038d57c0691b826aa" -->
#### 禁止手順: `FB-REC-106`

- 識別子: `forbidden:FB-REC-106#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:362`
- 表題: 残量が足りないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `estimate_space_short()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_take()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-REC-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> 残量を見積もらないまま収録を始められない

（出典: `specs/requirements/recording-input.fsl:373`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-107#forbidden_trace" digest="sha256:f86877446428d2886d4fe013186059c288ef09a7c092d615d05a3c91b3617a54" -->
#### 禁止手順: `FB-REC-107`

- 識別子: `forbidden:FB-REC-107#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:373`
- 表題: 残量を見積もらないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_take()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> デバイスを失った状態では収録していない

（出典: `specs/requirements/recording-input.fsl:220`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutDevice#state_rule" digest="sha256:71511c506b0c0a594d36dbe7ddfed240c8efd51552d8c6c9176f633c6fbbf54b" -->
#### 状態不変条件: `NoRecordingWithoutDevice`

- 識別子: `property:invariant:NoRecordingWithoutDevice#state_rule`
- 出典: `specs/requirements/recording-input.fsl:221`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`device` が `Selected` でないならば、`recording` が `false` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録しているならストリームは開いている

（出典: `specs/requirements/recording-input.fsl:225`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingRequiresOpenStream#state_rule" digest="sha256:4f33d403ef40f75f9c24d76d7d98c990cc0dae39a4fe0847d158936030e0913f" -->
#### 状態不変条件: `RecordingRequiresOpenStream`

- 識別子: `property:invariant:RecordingRequiresOpenStream#state_rule`
- 出典: `specs/requirements/recording-input.fsl:226`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`recording` が `true` であるならば、`stream_open` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 入力が届いていない、または未判定のまま収録することはない

（出典: `specs/requirements/recording-input.fsl:230`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutLiveInput#state_rule" digest="sha256:32615ea4937c52fde4de28c3cc9d77867eb46dd7a34a5051fa38a05b19987325" -->
#### 状態不変条件: `NoRecordingWithoutLiveInput`

- 識別子: `property:invariant:NoRecordingWithoutLiveInput#state_rule`
- 出典: `specs/requirements/recording-input.fsl:231`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`recording` が `true` であるならば、`liveness` が `Alive` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 校正していないまま収録することはない

（出典: `specs/requirements/recording-input.fsl:235`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutCalibration#state_rule" digest="sha256:6c655cd051d0aec1f3ad89ce676a8126a2d27f741f5ce5f1433fb3706bcf2d5f" -->
#### 状態不変条件: `NoRecordingWithoutCalibration`

- 識別子: `property:invariant:NoRecordingWithoutCalibration#state_rule`
- 出典: `specs/requirements/recording-input.fsl:236`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`recording` が `true` であるならば、`gain` が `Calibrated` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 回り込みを確認しないままガイドを鳴らすことはない

（出典: `specs/requirements/recording-input.fsl:240`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:GuideRequiresLeakCheck#state_rule" digest="sha256:d10f16e0aef9526e6bd0d5c352389f44c3389263cfcacc6825318f16d61b043f" -->
#### 状態不変条件: `GuideRequiresLeakCheck`

- 識別子: `property:invariant:GuideRequiresLeakCheck#state_rule`
- 出典: `specs/requirements/recording-input.fsl:241`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`guide_enabled` が `true` であるならば、`leak_checked` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> テイクが確定するのは、入力が届いていると判定できているときだけ

（出典: `specs/requirements/recording-input.fsl:245`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:TakeOnlyWhileAlive#transition_rule" digest="sha256:79bd304074fee1c2182b2ebe3974e34738af39adde84c23fabfc468463c91e08" -->
#### 遷移条件: `TakeOnlyWhileAlive`

- 識別子: `property:trans:TakeOnlyWhileAlive#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:246`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`takes` が 遷移前の `takes` に等しくないならば、`liveness` が `Alive` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> アプリが OS 側のゲインを変えたなら、終了時に必ず戻している

（出典: `specs/requirements/recording-input.fsl:250`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:GainRestoredOnExit#state_rule" digest="sha256:ed9523deec1888cc2017f5eeab5dc2e5768e36e4604b12770ad7353a61807c73" -->
#### 状態不変条件: `GainRestoredOnExit`

- 識別子: `property:invariant:GainRestoredOnExit#state_rule`
- 出典: `specs/requirements/recording-input.fsl:251`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

（`app_exited` が `true` である、かつ、`os_gain_changed` が `true` である）ならば、`os_gain_restored` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-108

**要件原文（意図。形式意味との一致は人間が確認する）**

> 手順の提示は多くとも一度しか出ない

（出典: `specs/requirements/recording-input.fsl:210`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:PromptAtMostOnce#state_rule" digest="sha256:830533089075dcf7329ba320feedb6bc9dcb8fa31ba1dad324a73f3b4c9e322a" -->
#### 状態不変条件: `PromptAtMostOnce`

- 識別子: `property:invariant:PromptAtMostOnce#state_rule`
- 出典: `specs/requirements/recording-input.fsl:211`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`prompts_shown` が `1` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-3: 収録中なら、テイク数の上限にはまだ達していない

（出典: `specs/requirements/recording-input.fsl:215`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingImpliesRoom#state_rule" digest="sha256:204a1b5004fc4626f98b30589dc1548560f62223cd5cf11f5c757d889846d421" -->
#### 状態不変条件: `RecordingImpliesRoom`

- 識別子: `property:invariant:RecordingImpliesRoom#state_rule`
- 出典: `specs/requirements/recording-input.fsl:216`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`recording` が `true` であるならば、`takes` が `MAX_TAKES` より小さい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 入力デバイスは本人が明示的に選び、識別子でプロジェクトに固定する

（出典: `specs/requirements/recording-input.fsl:59`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:select_device#operation" digest="sha256:56af877b9eacfaecc09d728510f3dfda3c1038591e8794b532d8cdf08a429583" -->
#### 操作: `select_device`

- 識別子: `action:select_device#operation`
- 出典: `specs/requirements/recording-input.fsl:60`
- パラメータ: なし

操作 `select_device` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `device` が `NotSelected` である。
2. `app_exited` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `device` を `Selected` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> テイクが確定してもストリームを閉じない

（出典: `specs/requirements/recording-input.fsl:275`）

> 収録画面に入った時点でストリームを開き、テイクごとに開閉しない

（出典: `specs/requirements/recording-input.fsl:66`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:open_stream#operation" digest="sha256:6df35711ec261dab26ed0b58c2ee0d75fc6e3db4c72e82ac400fa2b7befe6bb0" -->
#### 操作: `open_stream`

- 識別子: `action:open_stream#operation`
- 出典: `specs/requirements/recording-input.fsl:67`
- パラメータ: なし

操作 `open_stream` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。
3. `stream_open` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `stream_open` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:StreamStaysOpenAcrossTakes#transition_rule" digest="sha256:32836cbf4d2ede4faefcfd03018555556e61721902c86b097a7a4e6332a0d3e9" -->
#### 遷移条件: `StreamStaysOpenAcrossTakes`

- 識別子: `property:trans:StreamStaysOpenAcrossTakes#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:276`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
takes != old(takes) => stream_open and old(stream_open)
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 無効化できない効果が残ることがある

（出典: `specs/requirements/recording-input.fsl:82`）

> 開いたストリームに適用中の効果を列挙し、無効化できたものは無効化する

（出典: `specs/requirements/recording-input.fsl:74`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:disable_all_effects#operation" digest="sha256:507015a6bc0bd9147629e94d6deaba4491aae8f4d4117b3538bc70848a01ed7f" -->
#### 操作: `disable_all_effects`

- 識別子: `action:disable_all_effects#operation`
- 出典: `specs/requirements/recording-input.fsl:75`
- パラメータ: なし

操作 `disable_all_effects` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `effects` が `Unknown` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `effects` を `Clean` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:some_effects_remain#operation" digest="sha256:495021baf10d0afbe5112579c17c112e60e0420098dcf2b24b6c095833dbce54" -->
#### 操作: `some_effects_remain`

- 識別子: `action:some_effects_remain#operation`
- 出典: `specs/requirements/recording-input.fsl:83`
- パラメータ: なし

操作 `some_effects_remain` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `effects` が `Unknown` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `effects` を `SomeRemain` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 手順が提示される経路が存在する

（出典: `specs/requirements/recording-input.fsl:300`）

> 手順の提示は収録中に出ない

（出典: `specs/requirements/recording-input.fsl:255`）

> 手順を提示するのは、無効化できない効果が残っているときだけ

（出典: `specs/requirements/recording-input.fsl:260`）

> 無効化できない効果が残ったまま、それでも収録へ進める

（出典: `specs/requirements/recording-input.fsl:280`）

> 無効化できない効果が残ったら、収録開始前に一度だけ手順を提示する

（出典: `specs/requirements/recording-input.fsl:90`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:show_prompt_once#operation" digest="sha256:1e42f7b9a80ee589120d279045a447fbd5a3b65e2aff777e3c8fd5e7c181492c" -->
#### 操作: `show_prompt_once`

- 識別子: `action:show_prompt_once#operation`
- 出典: `specs/requirements/recording-input.fsl:91`
- パラメータ: なし

操作 `show_prompt_once` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `effects` が `SomeRemain` である。
3. `prompts_shown` が `0` に等しい。
4. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `prompts_shown` を `prompts_shown + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:PromptCanBeShown#reachability_goal" digest="sha256:8ba64bcfa75cff6ae4d6c64bdf09fc559aa7583433c1ec4168a47bc74fa055e6" -->
#### 到達目標: `PromptCanBeShown`

- 識別子: `property:reachable:PromptCanBeShown#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:301`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`prompts_shown` が `1` に等しい。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RecordsWithRemainingEffects#reachability_goal" digest="sha256:46fc4007965e5f509e06d5dae3d4b9f02089ecdb89ecae36e6e72cfd42d92af3" -->
#### 到達目標: `RecordsWithRemainingEffects`

- 識別子: `property:reachable:RecordsWithRemainingEffects#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:281`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次のすべてが成立する。

1. `takes` が `0` より大きい。

2. `effects` が `SomeRemain` である。

3. `prompts_shown` が `1` に等しい。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:PromptNeverDuringRecording#transition_rule" digest="sha256:3f0061fd8b5daa4b50602040f663432ff4af6f77c4e510284dfb0700518a261d" -->
#### 遷移条件: `PromptNeverDuringRecording`

- 識別子: `property:trans:PromptNeverDuringRecording#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:256`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`prompts_shown` が 遷移前の `prompts_shown` に等しくないならば、`recording` が `false` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:PromptOnlyWhenEffectsRemain#transition_rule" digest="sha256:8e1449a561e41201946fbe966d2c289d132701bcbc722f9b6a8ab77acba709a6" -->
#### 遷移条件: `PromptOnlyWhenEffectsRemain`

- 識別子: `property:trans:PromptOnlyWhenEffectsRemain#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:261`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`prompts_shown` が 遷移前の `prompts_shown` に等しくないならば、`effects` が `SomeRemain` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> アプリが変更した OS 側のゲインは、終了時に変更前の値へ戻す

（出典: `specs/requirements/recording-input.fsl:202`）

> 入力レベルの校正は収録前の1回のセットアップ工程で、収録中は行わない

（出典: `specs/requirements/recording-input.fsl:99`）

> 収録中にゲインを変えない

（出典: `specs/requirements/recording-input.fsl:270`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:calibrate_gain#operation" digest="sha256:54100c9c63aa88fa03c42c22eb4ef2e53aa801046cb2c2dafc4248c755a17226" -->
#### 操作: `calibrate_gain`

- 識別子: `action:calibrate_gain#operation`
- 出典: `specs/requirements/recording-input.fsl:100`
- パラメータ: なし

操作 `calibrate_gain` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。
3. `recording` が `false` である。
4. `gain` が `NotCalibrated` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `gain` を `Calibrated` にする。
2. `os_gain_changed` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:exit_app#operation" digest="sha256:97326386ba918a89f307184205a80bbedda3f617b464f2d2814d831bc7475177" -->
#### 操作: `exit_app`

- 識別子: `action:exit_app#operation`
- 出典: `specs/requirements/recording-input.fsl:203`
- パラメータ: なし

操作 `exit_app` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `false` である。
2. `app_exited` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `app_exited` を `true` にする。
2. `os_gain_restored` を `os_gain_changed` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:GainFixedDuringRecording#transition_rule" digest="sha256:f756b7c732db3d08d5fdafda22fbb3e1b414972eac2a472a0db17e8477b70898" -->
#### 遷移条件: `GainFixedDuringRecording`

- 識別子: `property:trans:GainFixedDuringRecording#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:271`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
gain != old(gain) => not recording and not old(recording)
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> 入力が届いていないことは状態として記録される

（出典: `specs/requirements/recording-input.fsl:295`）

> 入力が届いていないと判定したら、デバイス選択へ戻す

（出典: `specs/requirements/recording-input.fsl:265`）

> 入力が届いていなければ収録を止め、テイクを作らずデバイス選択へ戻す

（出典: `specs/requirements/recording-input.fsl:117`）

> 入力経路の生死は収録開始後の冒頭で一度だけ判定する

（出典: `specs/requirements/recording-input.fsl:109`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:input_is_alive#operation" digest="sha256:794dd0bc1dc2fdf0b910f6d002be0f21faa10b31c125ca3e13e0bac3938f46c2" -->
#### 操作: `input_is_alive`

- 識別子: `action:input_is_alive#operation`
- 出典: `specs/requirements/recording-input.fsl:110`
- パラメータ: なし

操作 `input_is_alive` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `liveness` が `Unchecked` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `liveness` を `Alive` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:input_is_dead#operation" digest="sha256:4d97372743b30a0fa29edcdce67124b885bb590030f0bcf2c84025a899ea672c" -->
#### 操作: `input_is_dead`

- 識別子: `action:input_is_dead#operation`
- 出典: `specs/requirements/recording-input.fsl:118`
- パラメータ: なし

操作 `input_is_dead` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `liveness` が `Unchecked` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `liveness` を `Dead` にする。
2. `recording` を `false` にする。
3. `stream_open` を `false` にする。
4. `device` を `NotSelected` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:InputCanBeDead#reachability_goal" digest="sha256:ead122724ecdf472ee6594479973ec805a5111e2fcb9c8e4c6ceafe5431d344d" -->
#### 到達目標: `InputCanBeDead`

- 識別子: `property:reachable:InputCanBeDead#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:296`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`liveness` が `Dead` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:DeadInputReturnsToDeviceSelection#transition_rule" digest="sha256:2c51022e3e2cd3150f2ce4defefaa6e67ebb088380fea90d0ab7d9d41a73c4f5" -->
#### 遷移条件: `DeadInputReturnsToDeviceSelection`

- 識別子: `property:trans:DeadInputReturnsToDeviceSelection#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:266`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

（`liveness` が `Dead` である、かつ、遷移前の `liveness` が `Dead` でない）ならば、`device` が `NotSelected` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> ガイドを鳴らす前に、回り込みの有無を一度だけ確認する

（出典: `specs/requirements/recording-input.fsl:128`）

> 回り込みが無いと確認できたときだけガイドを鳴らす

（出典: `specs/requirements/recording-input.fsl:137`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:check_guide_leak#operation" digest="sha256:b9f6390314657ede898a56c900bbaee1bab3cf4405ed6cef7d6a2adb53c6af2d" -->
#### 操作: `check_guide_leak`

- 識別子: `action:check_guide_leak#operation`
- 出典: `specs/requirements/recording-input.fsl:129`
- パラメータ: なし

操作 `check_guide_leak` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `leak_checked` が `false` である。
4. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `leak_checked` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:enable_guide#operation" digest="sha256:81b7544fe0cd934a2a7d90bda1e6201382e810e3678115103bc707733f54ff20" -->
#### 操作: `enable_guide`

- 識別子: `action:enable_guide#operation`
- 出典: `specs/requirements/recording-input.fsl:138`
- パラメータ: なし

操作 `enable_guide` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `leak_checked` が `true` である。
3. `guide_enabled` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `guide_enabled` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-108

**要件原文（意図。形式意味との一致は人間が確認する）**

> テイクが確定してもストリームは開いたままにする

（出典: `specs/requirements/recording-input.fsl:177`）

> 収録は、デバイスが生きていて入力が届いているときだけ始められる

（出典: `specs/requirements/recording-input.fsl:162`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finish_take#operation" digest="sha256:69794fc369050715d86bd08c55e738a13fdc791636de46ea95792f7bbc2be980" -->
#### 操作: `finish_take`

- 識別子: `action:finish_take#operation`
- 出典: `specs/requirements/recording-input.fsl:178`
- パラメータ: なし

操作 `finish_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `recording` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `false` にする。
2. `takes` を `takes + 1` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:0e80b60496c5a2412216a94cbe53ab5777d7d0b36307dfe02b709afa9642a32a" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/requirements/recording-input.fsl:164`
- パラメータ: なし

操作 `start_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。
3. `stream_open` が `true` である。
4. `liveness` が `Alive` である。
5. `gain` が `Calibrated` である。
6. `space_estimated` が `true` である。
7. `space_sufficient` が `true` である。
8. `recording` が `false` である。
9. `takes` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-109

**要件原文（意図。形式意味との一致は人間が確認する）**

> デバイスが戻れば収録を再開できる

（出典: `specs/requirements/recording-input.fsl:285`）

> デバイスの消失は状態として記録される

（出典: `specs/requirements/recording-input.fsl:290`）

> 復帰は同一識別子のデバイスが戻ったときだけ。別のデバイスへ自動で切り替えない

（出典: `specs/requirements/recording-input.fsl:195`）

> 選択済みデバイスが録音中に消失したら、進行中テイクを破棄して収録を止める

（出典: `specs/requirements/recording-input.fsl:185`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:device_lost#operation" digest="sha256:0bf19602aa67569d4d8a57bbb046ba4181be34febbf2bbd71fe1e59bb3ba8ab8" -->
#### 操作: `device_lost`

- 識別子: `action:device_lost#operation`
- 出典: `specs/requirements/recording-input.fsl:186`
- パラメータ: なし

操作 `device_lost` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `device` を `Lost` にする。
2. `recording` を `false` にする。
3. `stream_open` を `false` にする。
4. `liveness` を `Unchecked` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:same_device_returned#operation" digest="sha256:f27e0492b49efb0ff8607a7ffe81e876a43574d7410311ff4d389a05084998ff" -->
#### 操作: `same_device_returned`

- 識別子: `action:same_device_returned#operation`
- 出典: `specs/requirements/recording-input.fsl:196`
- パラメータ: なし

操作 `same_device_returned` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Lost` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `device` を `Selected` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:DeviceCanBeLost#reachability_goal" digest="sha256:414d6146ff868e42db5ddfbfd1c022660ba3da76e087819e660b4683055610ad" -->
#### 到達目標: `DeviceCanBeLost`

- 識別子: `property:reachable:DeviceCanBeLost#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:291`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`device` が `Lost` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ResumesAfterDeviceReturn#reachability_goal" digest="sha256:6ebb31fdebc4e8cd0129d48b42aca893f08b9311502aa22d005a800fa10715fb" -->
#### 到達目標: `ResumesAfterDeviceReturn`

- 識別子: `property:reachable:ResumesAfterDeviceReturn#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:286`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`takes` が `0` より大きい、かつ、`device` が `Selected` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-110

**要件原文（意図。形式意味との一致は人間が確認する）**

> 収録を始める前に、リスト全体が必要とする容量を見積もる

（出典: `specs/requirements/recording-input.fsl:146`）

> 残量が足りないと分かっている間は収録を始めない

（出典: `specs/requirements/recording-input.fsl:163`）

> 足りないと分かったら、録りきれる件数を提示して開始させない

（出典: `specs/requirements/recording-input.fsl:154`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:estimate_space_enough#operation" digest="sha256:34976729e7265f3b3ee1dbf0adbfac3325b8155cfdb993d8094b874c60c80d6f" -->
#### 操作: `estimate_space_enough`

- 識別子: `action:estimate_space_enough#operation`
- 出典: `specs/requirements/recording-input.fsl:147`
- パラメータ: なし

操作 `estimate_space_enough` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `space_estimated` を `true` にする。
2. `space_sufficient` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:estimate_space_short#operation" digest="sha256:49ebf168d3374c446a429954722e2bdd63653bd2dea2309213da51bba4387c8c" -->
#### 操作: `estimate_space_short`

- 識別子: `action:estimate_space_short#operation`
- 出典: `specs/requirements/recording-input.fsl:155`
- パラメータ: なし

操作 `estimate_space_short` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `space_estimated` を `true` にする。
2. `space_sufficient` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

この操作の内容は、`REQ-REC-108` の節に記載している。この要件にも同じ意味で適用される。

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:0a9bb4394b70944ed0b36b6b5164756a7ae8080158921d71c6f334039eaf4c1e" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

`app_exited` が `true` である。

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/recording-input.fsl`（`KoeruRecordingInput`、dialect: `requirements`）
- spec digest: `sha256:a5d5a1438298543f27c6dc08719bbedcc639d94388a9c51555a7c9d58ef83946`
- claim set digest: `sha256:edb9464e528fd4bc9e54fe8deeb350fbb82e42a9a82f139492dc3e37ae1a2578`
- 形式要素の分類: rendered 44 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 2 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 2 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
