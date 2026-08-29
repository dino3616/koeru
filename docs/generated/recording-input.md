---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/recording-input.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:42607b3df78fc84f257d9bc9c61034157b212984c5dab3743ee3e70feb2a45b3
claim_set_digest: sha256:2ffb625fb2481b5edbb6e47fde0aebc1246b628d22e56d20b8cb55955c23318e
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

> 選んで、開いて、効果を無効化し、校正し、生死を確かめて録る

（出典: `specs/requirements/recording-input.fsl:281`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-REC-101#acceptance_trace" digest="sha256:c26c3c2e6ea2df70bb29973cce41d965c7bd681fee549e6806adfd27f013b68a" -->
#### 受け入れ基準: `AC-REC-101`

- 識別子: `acceptance:AC-REC-101#acceptance_trace`
- 出典: `specs/requirements/recording-input.fsl:281`
- 表題: 選んで、開いて、効果を無効化し、校正し、生死を確かめて録る

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `start_take()`
  7. `finish_take()`
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

（出典: `specs/requirements/recording-input.fsl:292`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-101#forbidden_trace" digest="sha256:1f16f0293ae64b75437d3ee04f78d54b26d4c3b79e07e56e736c17bc2da6a8bb" -->
#### 禁止手順: `FB-REC-101`

- 識別子: `forbidden:FB-REC-101#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:292`
- 表題: デバイスが消失したまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `device_lost()`
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

（出典: `specs/requirements/recording-input.fsl:303`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-102#forbidden_trace" digest="sha256:b95b657dc48ae57977e8ddba0b36d640f13d87dc1cae696828a766df6c3fefb5" -->
#### 禁止手順: `FB-REC-102`

- 識別子: `forbidden:FB-REC-102#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:303`
- 表題: 入力が届いていないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `calibrate_gain()`
  5. `input_is_dead()`
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

（出典: `specs/requirements/recording-input.fsl:313`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-103#forbidden_trace" digest="sha256:fa7e6830cfe5b8488b822549b807f40ef71be6b62c54254b5db79f2761d4685a" -->
#### 禁止手順: `FB-REC-103`

- 識別子: `forbidden:FB-REC-103#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:313`
- 表題: 校正しないまま収録を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `disable_all_effects()`
  4. `input_is_alive()`
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

（出典: `specs/requirements/recording-input.fsl:322`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-104#forbidden_trace" digest="sha256:9bbd5b0f204372794e320b27fb2e95ed5ba2315c9f507f9961367bc255912944" -->
#### 禁止手順: `FB-REC-104`

- 識別子: `forbidden:FB-REC-104#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:322`
- 表題: 収録中に手順の提示を出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `select_device()`
  2. `open_stream()`
  3. `some_effects_remain()`
  4. `calibrate_gain()`
  5. `input_is_alive()`
  6. `start_take()`
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

（出典: `specs/requirements/recording-input.fsl:333`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-REC-105#forbidden_trace" digest="sha256:404b3928c6a732c4475950ddf1e82c390516e7b65a12faf053375a977436b05b" -->
#### 禁止手順: `FB-REC-105`

- 識別子: `forbidden:FB-REC-105#forbidden_trace`
- 出典: `specs/requirements/recording-input.fsl:333`
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

### INV-REC-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> デバイスを失った状態では収録していない

（出典: `specs/requirements/recording-input.fsl:196`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutDevice#state_rule" digest="sha256:9873b2225b0c2684789413a28d78a2f79d7a42d1b3ef366fb651797c78dfa2b0" -->
#### 状態不変条件: `NoRecordingWithoutDevice`

- 識別子: `property:invariant:NoRecordingWithoutDevice#state_rule`
- 出典: `specs/requirements/recording-input.fsl:197`

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

（出典: `specs/requirements/recording-input.fsl:201`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingRequiresOpenStream#state_rule" digest="sha256:03fc79265c5ea9e71aca945c7619d5d91de60be9c4d76f812989e1e2b9b4a5f5" -->
#### 状態不変条件: `RecordingRequiresOpenStream`

- 識別子: `property:invariant:RecordingRequiresOpenStream#state_rule`
- 出典: `specs/requirements/recording-input.fsl:202`

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

（出典: `specs/requirements/recording-input.fsl:206`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutLiveInput#state_rule" digest="sha256:adc6cb4c218494cdb305fc6a150f41d636fb8e8dc81b4c0158b53b4b037a309f" -->
#### 状態不変条件: `NoRecordingWithoutLiveInput`

- 識別子: `property:invariant:NoRecordingWithoutLiveInput#state_rule`
- 出典: `specs/requirements/recording-input.fsl:207`

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

（出典: `specs/requirements/recording-input.fsl:211`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoRecordingWithoutCalibration#state_rule" digest="sha256:469d85c94f74b45fa1431d87a0c0742bee8f92533f0470ac3a48bb659b4aa097" -->
#### 状態不変条件: `NoRecordingWithoutCalibration`

- 識別子: `property:invariant:NoRecordingWithoutCalibration#state_rule`
- 出典: `specs/requirements/recording-input.fsl:212`

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

（出典: `specs/requirements/recording-input.fsl:216`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:GuideRequiresLeakCheck#state_rule" digest="sha256:0bfd15bb0944f9dc0da869a702f5bd0053243886f03e5c89db4070ef8868bd12" -->
#### 状態不変条件: `GuideRequiresLeakCheck`

- 識別子: `property:invariant:GuideRequiresLeakCheck#state_rule`
- 出典: `specs/requirements/recording-input.fsl:217`

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

（出典: `specs/requirements/recording-input.fsl:221`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:TakeOnlyWhileAlive#transition_rule" digest="sha256:7cc4f4e091ae758e1f6d1bf820b2286d8ba48fd7288d8a4cc77bc7f90b596868" -->
#### 遷移条件: `TakeOnlyWhileAlive`

- 識別子: `property:trans:TakeOnlyWhileAlive#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:222`

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

（出典: `specs/requirements/recording-input.fsl:226`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:GainRestoredOnExit#state_rule" digest="sha256:05a210eaee6b3d3112b46ba7a90617072319dd4e7282eaebac268df83247f39a" -->
#### 状態不変条件: `GainRestoredOnExit`

- 識別子: `property:invariant:GainRestoredOnExit#state_rule`
- 出典: `specs/requirements/recording-input.fsl:227`

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

（出典: `specs/requirements/recording-input.fsl:186`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:PromptAtMostOnce#state_rule" digest="sha256:7742abd6fe703d7c1353b1a12eaa7288033f14db6d71e00a5926acd32d0e6b83" -->
#### 状態不変条件: `PromptAtMostOnce`

- 識別子: `property:invariant:PromptAtMostOnce#state_rule`
- 出典: `specs/requirements/recording-input.fsl:187`

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

（出典: `specs/requirements/recording-input.fsl:191`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RecordingImpliesRoom#state_rule" digest="sha256:5a560b45ad70d177921436bc5b9a6ebafe8a8f495ba618050e831898accf0c4f" -->
#### 状態不変条件: `RecordingImpliesRoom`

- 識別子: `property:invariant:RecordingImpliesRoom#state_rule`
- 出典: `specs/requirements/recording-input.fsl:192`

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

（出典: `specs/requirements/recording-input.fsl:55`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:select_device#operation" digest="sha256:50963ed24dbe29c6ca34a3ef80bab05d1417623bad4400900566385267a4ff9b" -->
#### 操作: `select_device`

- 識別子: `action:select_device#operation`
- 出典: `specs/requirements/recording-input.fsl:56`
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

（出典: `specs/requirements/recording-input.fsl:251`）

> 収録画面に入った時点でストリームを開き、テイクごとに開閉しない

（出典: `specs/requirements/recording-input.fsl:62`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:open_stream#operation" digest="sha256:b885cae3262d4cb3cc788d6bdeb703685444389db1e10fe86349b95ff3d0b2ac" -->
#### 操作: `open_stream`

- 識別子: `action:open_stream#operation`
- 出典: `specs/requirements/recording-input.fsl:63`
- パラメータ: なし

操作 `open_stream` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。
3. `stream_open` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `stream_open` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:StreamStaysOpenAcrossTakes#transition_rule" digest="sha256:114cff7d485ad3bbefd85d2227aa616673e94e49e862d8f612c28cf00408a6b5" -->
#### 遷移条件: `StreamStaysOpenAcrossTakes`

- 識別子: `property:trans:StreamStaysOpenAcrossTakes#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:252`

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

（出典: `specs/requirements/recording-input.fsl:78`）

> 開いたストリームに適用中の効果を列挙し、無効化できたものは無効化する

（出典: `specs/requirements/recording-input.fsl:70`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:disable_all_effects#operation" digest="sha256:f211fe1a3239f3401053d9833833f3ba6fe9863a2fb2576357d90d781e159c57" -->
#### 操作: `disable_all_effects`

- 識別子: `action:disable_all_effects#operation`
- 出典: `specs/requirements/recording-input.fsl:71`
- パラメータ: なし

操作 `disable_all_effects` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `effects` が `Unknown` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `effects` を `Clean` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:some_effects_remain#operation" digest="sha256:d55a1f7fcf815f52ae28bab66f02b4ffbf61771831672188b6ba59d80546cbbc" -->
#### 操作: `some_effects_remain`

- 識別子: `action:some_effects_remain#operation`
- 出典: `specs/requirements/recording-input.fsl:79`
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

（出典: `specs/requirements/recording-input.fsl:276`）

> 手順の提示は収録中に出ない

（出典: `specs/requirements/recording-input.fsl:231`）

> 手順を提示するのは、無効化できない効果が残っているときだけ

（出典: `specs/requirements/recording-input.fsl:236`）

> 無効化できない効果が残ったまま、それでも収録へ進める

（出典: `specs/requirements/recording-input.fsl:256`）

> 無効化できない効果が残ったら、収録開始前に一度だけ手順を提示する

（出典: `specs/requirements/recording-input.fsl:86`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:show_prompt_once#operation" digest="sha256:40d1f6c671b670e65ee9f3b0e433c965a95b4668ded1a99432f7b0ef53bd9fc3" -->
#### 操作: `show_prompt_once`

- 識別子: `action:show_prompt_once#operation`
- 出典: `specs/requirements/recording-input.fsl:87`
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

<!-- fsl:claim begin id="property:reachable:PromptCanBeShown#reachability_goal" digest="sha256:0bb3f7f2e05bf65e59bc9cb3fc9daf5e5bfa9137abb1353de2a62ac0ad1c9d3b" -->
#### 到達目標: `PromptCanBeShown`

- 識別子: `property:reachable:PromptCanBeShown#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:277`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`prompts_shown` が `1` に等しい。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RecordsWithRemainingEffects#reachability_goal" digest="sha256:8eb491f40c72426fcb820b3243b83ffb2bf40ef8afd2aff5f79aa46f5d844b6c" -->
#### 到達目標: `RecordsWithRemainingEffects`

- 識別子: `property:reachable:RecordsWithRemainingEffects#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:257`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次のすべてが成立する。

1. `takes` が `0` より大きい。

2. `effects` が `SomeRemain` である。

3. `prompts_shown` が `1` に等しい。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:PromptNeverDuringRecording#transition_rule" digest="sha256:25d9395f43a41017937f6773e0c55ebd2c4b9a592f2c27e2938b659a80772f74" -->
#### 遷移条件: `PromptNeverDuringRecording`

- 識別子: `property:trans:PromptNeverDuringRecording#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:232`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`prompts_shown` が 遷移前の `prompts_shown` に等しくないならば、`recording` が `false` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:PromptOnlyWhenEffectsRemain#transition_rule" digest="sha256:a4043df32a3f6867164e96e432037ace6c189e300069657ad38624463a93f3ff" -->
#### 遷移条件: `PromptOnlyWhenEffectsRemain`

- 識別子: `property:trans:PromptOnlyWhenEffectsRemain#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:237`

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

（出典: `specs/requirements/recording-input.fsl:178`）

> 入力レベルの校正は収録前の1回のセットアップ工程で、収録中は行わない

（出典: `specs/requirements/recording-input.fsl:95`）

> 収録中にゲインを変えない

（出典: `specs/requirements/recording-input.fsl:246`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:calibrate_gain#operation" digest="sha256:8837ca967156b4dc6e81530a6be3a2f5e18709daf1e711f0dad59e59440138f6" -->
#### 操作: `calibrate_gain`

- 識別子: `action:calibrate_gain#operation`
- 出典: `specs/requirements/recording-input.fsl:96`
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

<!-- fsl:claim begin id="action:exit_app#operation" digest="sha256:a51d2fb2885a1fe69b9997607566a547e241a8a3552f70e6b591e4f4aa56fec4" -->
#### 操作: `exit_app`

- 識別子: `action:exit_app#operation`
- 出典: `specs/requirements/recording-input.fsl:179`
- パラメータ: なし

操作 `exit_app` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `false` である。
2. `app_exited` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `app_exited` を `true` にする。
2. `os_gain_restored` を `os_gain_changed` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:GainFixedDuringRecording#transition_rule" digest="sha256:1a8ac5bf3c1f82b9a476817074e9007fefc2797adfc92d2fd0a03620c4871768" -->
#### 遷移条件: `GainFixedDuringRecording`

- 識別子: `property:trans:GainFixedDuringRecording#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:247`

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

（出典: `specs/requirements/recording-input.fsl:271`）

> 入力が届いていないと判定したら、デバイス選択へ戻す

（出典: `specs/requirements/recording-input.fsl:241`）

> 入力が届いていなければ収録を止め、テイクを作らずデバイス選択へ戻す

（出典: `specs/requirements/recording-input.fsl:113`）

> 入力経路の生死は収録開始後の冒頭で一度だけ判定する

（出典: `specs/requirements/recording-input.fsl:105`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:input_is_alive#operation" digest="sha256:2dbf1e6a882f9729dc532002150ff66ecbfd2f9b6f0f4fe1e50212491950c769" -->
#### 操作: `input_is_alive`

- 識別子: `action:input_is_alive#operation`
- 出典: `specs/requirements/recording-input.fsl:106`
- パラメータ: なし

操作 `input_is_alive` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `stream_open` が `true` である。
3. `liveness` が `Unchecked` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `liveness` を `Alive` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:input_is_dead#operation" digest="sha256:0ae1f4cf8f1ec5c4b422ce7912861ce51e30af5a6907544b486e92502ec0219f" -->
#### 操作: `input_is_dead`

- 識別子: `action:input_is_dead#operation`
- 出典: `specs/requirements/recording-input.fsl:114`
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

<!-- fsl:claim begin id="property:reachable:InputCanBeDead#reachability_goal" digest="sha256:5589dd2582817026b345014b3db56ddc466da38d3f32bf63aa4d2bd20054fd49" -->
#### 到達目標: `InputCanBeDead`

- 識別子: `property:reachable:InputCanBeDead#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:272`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`liveness` が `Dead` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:DeadInputReturnsToDeviceSelection#transition_rule" digest="sha256:5d7df639aaea98a48b3821486839f3f123c3eaaeed8d90600a0e2257e714ac59" -->
#### 遷移条件: `DeadInputReturnsToDeviceSelection`

- 識別子: `property:trans:DeadInputReturnsToDeviceSelection#transition_rule`
- 出典: `specs/requirements/recording-input.fsl:242`

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

（出典: `specs/requirements/recording-input.fsl:124`）

> 回り込みが無いと確認できたときだけガイドを鳴らす

（出典: `specs/requirements/recording-input.fsl:133`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:check_guide_leak#operation" digest="sha256:e456073f0103b45681282ad88752c223a2f091cbcfb56df0ddbe107c529173f4" -->
#### 操作: `check_guide_leak`

- 識別子: `action:check_guide_leak#operation`
- 出典: `specs/requirements/recording-input.fsl:125`
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

<!-- fsl:claim begin id="action:enable_guide#operation" digest="sha256:20ace266208499d8cf871a4209924c3df8ce7bcb788dd5baf0de1c9e31125e7d" -->
#### 操作: `enable_guide`

- 識別子: `action:enable_guide#operation`
- 出典: `specs/requirements/recording-input.fsl:134`
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

（出典: `specs/requirements/recording-input.fsl:153`）

> 収録は、デバイスが生きていて入力が届いているときだけ始められる

（出典: `specs/requirements/recording-input.fsl:141`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finish_take#operation" digest="sha256:dafc3f30caaadf42e63d50d6e4ac923b6ff131a5ec8a2668601fa99866058ffd" -->
#### 操作: `finish_take`

- 識別子: `action:finish_take#operation`
- 出典: `specs/requirements/recording-input.fsl:154`
- パラメータ: なし

操作 `finish_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `recording` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `false` にする。
2. `takes` を `takes + 1` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:adb1f0958a17bce303792ead779454948a16b2965b007f7976df608bb306476b" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/requirements/recording-input.fsl:142`
- パラメータ: なし

操作 `start_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Selected` である。
3. `stream_open` が `true` である。
4. `liveness` が `Alive` である。
5. `gain` が `Calibrated` である。
6. `recording` が `false` である。
7. `takes` が `MAX_TAKES` より小さい。

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

（出典: `specs/requirements/recording-input.fsl:261`）

> デバイスの消失は状態として記録される

（出典: `specs/requirements/recording-input.fsl:266`）

> 復帰は同一識別子のデバイスが戻ったときだけ。別のデバイスへ自動で切り替えない

（出典: `specs/requirements/recording-input.fsl:171`）

> 選択済みデバイスが録音中に消失したら、進行中テイクを破棄して収録を止める

（出典: `specs/requirements/recording-input.fsl:161`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:device_lost#operation" digest="sha256:4e02dfa60c0836e5a0dbc68ba34c06123d9eb4c2f028d18fb92e0b24feac253b" -->
#### 操作: `device_lost`

- 識別子: `action:device_lost#operation`
- 出典: `specs/requirements/recording-input.fsl:162`
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

<!-- fsl:claim begin id="action:same_device_returned#operation" digest="sha256:a2c92924f2077265a0913064045d7d10e33d7e570acd9407f40d4042915052f8" -->
#### 操作: `same_device_returned`

- 識別子: `action:same_device_returned#operation`
- 出典: `specs/requirements/recording-input.fsl:172`
- パラメータ: なし

操作 `same_device_returned` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `app_exited` が `false` である。
2. `device` が `Lost` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `device` を `Selected` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:DeviceCanBeLost#reachability_goal" digest="sha256:66420bfe993c6ca036f2f17a649abf497282ab182fbf06e60b4dc2541a1bb720" -->
#### 到達目標: `DeviceCanBeLost`

- 識別子: `property:reachable:DeviceCanBeLost#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:267`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`device` が `Lost` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ResumesAfterDeviceReturn#reachability_goal" digest="sha256:5bad65292dab723844868bcd50eed057082c9d5b4183d48088519bf92b7de688" -->
#### 到達目標: `ResumesAfterDeviceReturn`

- 識別子: `property:reachable:ResumesAfterDeviceReturn#reachability_goal`
- 出典: `specs/requirements/recording-input.fsl:262`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`takes` が `0` より大きい、かつ、`device` が `Selected` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

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
- spec digest: `sha256:42607b3df78fc84f257d9bc9c61034157b212984c5dab3743ee3e70feb2a45b3`
- claim set digest: `sha256:2ffb625fb2481b5edbb6e47fde0aebc1246b628d22e56d20b8cb55955c23318e`
- 形式要素の分類: rendered 40 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 2 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 2 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
