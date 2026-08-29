---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/preview-synthesis.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:7f5a62f0b7dbd27b29a7899efbc16befa5cfb31e567207a766d8b24ec8c704e9
claim_set_digest: sha256:238b466611199206ab20b31ba0eff62d5d7bdc41fe8344bba6d132d552698ef6
---

# 要件仕様書: KoeruPreviewSynthesis

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

### AC-SYN-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> oto が動いたフレーズだけキャッシュを捨て、他は残す

（出典: `specs/requirements/preview-synthesis.fsl:148`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-SYN-101#acceptance_trace" digest="sha256:1d69156127468a6334e7edeb337b4443cf1d7af5198b981d8c3789007dd20aca" -->
#### 受け入れ基準: `AC-SYN-101`

- 識別子: `acceptance:AC-SYN-101#acceptance_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:148`
- 表題: oto が動いたフレーズだけキャッシュを捨て、他は残す

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `phrase_becomes_resolvable(1)`
  3. `start_render(0)`
  4. `finish_render()`
  5. `start_render(1)`
  6. `finish_render()`
  7. `oto_changed(0)`
- 期待（Then）: 最後の操作のあと、`cached[0]` が `false` である、かつ、`cached[1]` が `true` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-SYN-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成コアが変わると、すべてのキャッシュを捨てる

（出典: `specs/requirements/preview-synthesis.fsl:159`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-SYN-102#acceptance_trace" digest="sha256:9c12e44fa2087584b954b165749adcaa9125da13999dfdc59d7d0b9211617a7e" -->
#### 受け入れ基準: `AC-SYN-102`

- 識別子: `acceptance:AC-SYN-102#acceptance_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:159`
- 表題: 合成コアが変わると、すべてのキャッシュを捨てる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `phrase_becomes_resolvable(1)`
  3. `start_render(0)`
  4. `finish_render()`
  5. `start_render(1)`
  6. `finish_render()`
  7. `core_upgraded()`
- 期待（Then）: 最後の操作のあと、`cached[0]` が `false` である、かつ、`cached[1]` が `false` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-SYN-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 中断したフレーズはキャッシュに載らない

（出典: `specs/requirements/preview-synthesis.fsl:170`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-SYN-103#acceptance_trace" digest="sha256:b85c29848c3309226f1d45d60caf603d1c49573a9dec453ea4449023e4f7e34c" -->
#### 受け入れ基準: `AC-SYN-103`

- 識別子: `acceptance:AC-SYN-103#acceptance_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:170`
- 表題: 中断したフレーズはキャッシュに載らない

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `start_render(0)`
  3. `cancel_render()`
- 期待（Then）: 最後の操作のあと、`cached[0]` が `false` である、かつ、`rendering` が `none` に等しい。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-SYN-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 録り直したフレーズのキャッシュだけを捨て、他は残す

（出典: `specs/requirements/preview-synthesis.fsl:177`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-SYN-104#acceptance_trace" digest="sha256:b469a20c1774b7a22ca1400a418ba2bef628aeb81c4a68f4d156f7bb33bc2193" -->
#### 受け入れ基準: `AC-SYN-104`

- 識別子: `acceptance:AC-SYN-104#acceptance_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:177`
- 表題: 録り直したフレーズのキャッシュだけを捨て、他は残す

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `phrase_becomes_resolvable(1)`
  3. `start_render(0)`
  4. `finish_render()`
  5. `start_render(1)`
  6. `finish_render()`
  7. `take_rerecorded(1)`
- 期待（Then）: 最後の操作のあと、`cached[0]` が `true` である、かつ、`cached[1]` が `false` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-SYN-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 鳴らせる長さが揃えば、合成済みのフレーズから再生を始められる

（出典: `specs/requirements/preview-synthesis.fsl:188`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-SYN-105#acceptance_trace" digest="sha256:3e92c2f360d559e8cfdd6881c7a7aa7377391c9922c83402ef6675a92ef1af67" -->
#### 受け入れ基準: `AC-SYN-105`

- 識別子: `acceptance:AC-SYN-105#acceptance_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:188`
- 表題: 鳴らせる長さが揃えば、合成済みのフレーズから再生を始められる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `phrase_becomes_resolvable(1)`
  3. `start_render(0)`
  4. `finish_render()`
  5. `start_preview(0)`
- 期待（Then）: 最後の操作のあと、`playing` が `true` である、かつ、`resolvable[2]` が `false` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-SYN-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 鳴らせる長さが足りないうちは試唱を始められない

（出典: `specs/requirements/preview-synthesis.fsl:197`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-SYN-101#forbidden_trace" digest="sha256:6fa4cb4f706a8b38fcff582c266c8f314da8339a17202dde0b6248a765116465" -->
#### 禁止手順: `FB-SYN-101`

- 識別子: `forbidden:FB-SYN-101#forbidden_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:197`
- 表題: 鳴らせる長さが足りないうちは試唱を始められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `start_render(0)`
  3. `finish_render()`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_preview(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-SYN-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 解決できないフレーズは合成しない

（出典: `specs/requirements/preview-synthesis.fsl:205`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-SYN-102#forbidden_trace" digest="sha256:79fa283a65a699f4930948ff93b5a7f9bc5e0e4e835efa0e29ad193845b1f91a" -->
#### 禁止手順: `FB-SYN-102`

- 識別子: `forbidden:FB-SYN-102#forbidden_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:205`
- 表題: 解決できないフレーズは合成しない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 先行する操作はない。初期化直後の状態で、次の操作を試みる。
- 期待（Then）: 続けて実行しようとする最後の操作 `start_render(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-SYN-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成が終わっていないフレーズから再生を始めない

（出典: `specs/requirements/preview-synthesis.fsl:210`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-SYN-103#forbidden_trace" digest="sha256:c0900502a518c1ba86f7b80d511dc815f705695d0df7b2587740a5909e921da9" -->
#### 禁止手順: `FB-SYN-103`

- 識別子: `forbidden:FB-SYN-103#forbidden_trace`
- 出典: `specs/requirements/preview-synthesis.fsl:210`
- 表題: 合成が終わっていないフレーズから再生を始めない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `phrase_becomes_resolvable(0)`
  2. `phrase_becomes_resolvable(1)`
- 期待（Then）: 続けて実行しようとする最後の操作 `start_preview(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-SYN-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 鳴らせる長さが足りない曲は試唱に出さない

（出典: `specs/requirements/preview-synthesis.fsl:108`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoPreviewBelowThreshold#state_rule" digest="sha256:3aeea76824a1006278c2e3cd551fa05a4c36eda821c5dd3cd5a51aecb9264102" -->
#### 状態不変条件: `NoPreviewBelowThreshold`

- 識別子: `property:invariant:NoPreviewBelowThreshold#state_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:109`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`playing` が `true` であるならば、「`resolvable[p]` が `true` である」を満たす `p: Phrase` の個数 が `MIN_PLAYABLE` 以上である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-SYN-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成中のフレーズは、まだキャッシュに載っていない

（出典: `specs/requirements/preview-synthesis.fsl:113`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RenderingIsNotCached#state_rule" digest="sha256:5a3f826f626bc9cba0e0d7e2c34f4cad30715e8fec09031a36a752dd1b93adb1" -->
#### 状態不変条件: `RenderingIsNotCached`

- 識別子: `property:invariant:RenderingIsNotCached#state_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:114`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `p: Phrase` について、`rendering` が `some(p)` に等しいならば、`cached[p]` が `false` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-SYN-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> キャッシュに載っているのは、解決できるフレーズだけ

（出典: `specs/requirements/preview-synthesis.fsl:123`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:CachedImpliesResolvable#state_rule" digest="sha256:adf1c4e0db2e59289497e912198c68f823f93e0cb9d5728092905e5e5aa77d35" -->
#### 状態不変条件: `CachedImpliesResolvable`

- 識別子: `property:invariant:CachedImpliesResolvable#state_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:124`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `p: Phrase` について、`cached[p]` が `true` であるならば、`resolvable[p]` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-SYN-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成しているのは、解決できるフレーズだけ

（出典: `specs/requirements/preview-synthesis.fsl:118`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:RenderingImpliesResolvable#state_rule" digest="sha256:a2d63a42622ae6d4e558fb36a064e1f89ddbd8dc5b9234a291caca8676d6b705" -->
#### 状態不変条件: `RenderingImpliesResolvable`

- 識別子: `property:invariant:RenderingImpliesResolvable#state_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:119`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `p: Phrase` について、`rendering` が `some(p)` に等しいならば、`resolvable[p]` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 解決できるようになったフレーズは、そのあと解決できなくならない

（出典: `specs/requirements/preview-synthesis.fsl:133`）

> 録り足すと、解決できるフレーズが増える

（出典: `specs/requirements/preview-synthesis.fsl:39`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:phrase_becomes_resolvable#operation" digest="sha256:bb844fd0c73103fe012dac65908eac5cf45c2485685c4d655aa59697572b1f51" -->
#### 操作: `phrase_becomes_resolvable`

- 識別子: `action:phrase_becomes_resolvable#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:40`
- パラメータ: `p: Phrase`

操作 `phrase_becomes_resolvable` を実行できるのは、次の条件を満たす場合に限る。

1. `resolvable[p]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `resolvable[p]` を `true` にする。
2. `last_step_cancelled` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ResolvableIsMonotone#transition_rule" digest="sha256:654a09f3d1223bf8268d814a7cd809edf906f246c87c63eb10188e6fb574e43f" -->
#### 遷移条件: `ResolvableIsMonotone`

- 識別子: `property:trans:ResolvableIsMonotone#transition_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:134`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall p: Phrase { old(resolvable[p]) => resolvable[p] }
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成が終わったフレーズはキャッシュに載る

（出典: `specs/requirements/preview-synthesis.fsl:55`）

> 解決できるフレーズだけを合成する

（出典: `specs/requirements/preview-synthesis.fsl:46`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finish_render#operation" digest="sha256:924de4a8e2816aecc3215ba4aa5c26e710fc29a322818a2d9913a3a64c0f7e8a" -->
#### 操作: `finish_render`

- 識別子: `action:finish_render#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:56`
- パラメータ: なし

操作 `finish_render` を実行できるのは、次の条件を満たす場合に限る。

1. `rendering` が `some` である（その値を `p` と呼ぶ）。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `rendering` を `none` にする。
2. `cached[p]` を `true` にする。
3. `last_step_cancelled` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:start_render#operation" digest="sha256:c6ef9c0d9644e2e0c60b5c759a038c912ba164d450e9390db68cfa4b5f07bc00" -->
#### 操作: `start_render`

- 識別子: `action:start_render#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:47`
- パラメータ: `p: Phrase`

操作 `start_render` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `rendering` が `none` に等しい。
2. `resolvable[p]` が `true` である。
3. `cached[p]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `rendering` を `some(p)` にする。
2. `last_step_cancelled` を `false` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 中断したフレーズの部分結果は捨て、キャッシュへ書き込まない

（出典: `specs/requirements/preview-synthesis.fsl:63`）

> 中断した手は、キャッシュを増やさない

（出典: `specs/requirements/preview-synthesis.fsl:128`）

> 中断できる経路が存在する

（出典: `specs/requirements/preview-synthesis.fsl:143`）

> 試唱を止めると、進行中の合成も中断する

（出典: `specs/requirements/preview-synthesis.fsl:79`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:cancel_render#operation" digest="sha256:4aab066fdb6c0bcd5b420f2cff7239af5e8adfe3e1ba61b22625c07bfc9ffa4d" -->
#### 操作: `cancel_render`

- 識別子: `action:cancel_render#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:64`
- パラメータ: なし

操作 `cancel_render` を実行できるのは、次の条件を満たす場合に限る。

1. `rendering` が `none` に等しくない。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `rendering` を `none` にする。
2. `last_step_cancelled` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:stop_preview#operation" digest="sha256:98e086fb131084107ae93ab107db8092df4c878254d6e155e90b928837164359" -->
#### 操作: `stop_preview`

- 識別子: `action:stop_preview#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:80`
- パラメータ: なし

操作 `stop_preview` を実行できるのは、次の条件を満たす場合に限る。

1. `playing` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `playing` を `false` にする。
2. `rendering` を `none` にする。
3. `last_step_cancelled` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RenderCanBeCancelled#reachability_goal" digest="sha256:f10ed47298e5f0ffc2e28f8d4f2c909da4b993e6c2d3c29afc7db2e18a05ece1" -->
#### 到達目標: `RenderCanBeCancelled`

- 識別子: `property:reachable:RenderCanBeCancelled#reachability_goal`
- 出典: `specs/requirements/preview-synthesis.fsl:144`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`last_step_cancelled` が `true` である、かつ、`rendering` が `none` に等しい。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:CancelNeverWritesCache#transition_rule" digest="sha256:9b17414d9226ce6b6265143013d2546bf65bae3b8a95db9ff99057c582a251d6" -->
#### 遷移条件: `CancelNeverWritesCache`

- 識別子: `property:trans:CancelNeverWritesCache#transition_rule`
- 出典: `specs/requirements/preview-synthesis.fsl:129`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
last_step_cancelled => (forall p: Phrase { cached[p] == old(cached[p]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 先頭フレーズの合成が終わってから再生を始める

（出典: `specs/requirements/preview-synthesis.fsl:70`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:start_preview#operation" digest="sha256:130f31e682613f9956bc0f8c63c781968314a81c3a545f6bafc6ead3656d6d35" -->
#### 操作: `start_preview`

- 識別子: `action:start_preview#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:71`
- パラメータ: `p: Phrase`

操作 `start_preview` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `playing` が `false` である。
2. 「`resolvable[p]` が `true` である」を満たす `p: Phrase` の個数 が `MIN_PLAYABLE` 以上である。
3. `cached[p]` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `playing` を `true` にする。
2. `last_step_cancelled` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> oto の5値が動いたフレーズのキャッシュだけを捨てる

（出典: `specs/requirements/preview-synthesis.fsl:87`）

> 録り直したフレーズのキャッシュだけを捨てる

（出典: `specs/requirements/preview-synthesis.fsl:94`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:oto_changed#operation" digest="sha256:673b8b4e9ee89ae0704f65a12871a50d94a458d8d81fdc28fe7c4dab89fd3e4e" -->
#### 操作: `oto_changed`

- 識別子: `action:oto_changed#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:88`
- パラメータ: `p: Phrase`

操作 `oto_changed` を実行できるのは、次の条件を満たす場合に限る。

1. `cached[p]` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `cached[p]` を `false` にする。
2. `last_step_cancelled` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:take_rerecorded#operation" digest="sha256:8792335f99d0702ab09e1bd21bad1feae6067e2416f2b313495a70508a6a4cbe" -->
#### 操作: `take_rerecorded`

- 識別子: `action:take_rerecorded#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:95`
- パラメータ: `p: Phrase`

操作 `take_rerecorded` を実行できるのは、次の条件を満たす場合に限る。

1. `cached[p]` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `cached[p]` を `false` にする。
2. `last_step_cancelled` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> 合成コアが変わったら、すべてのフレーズのキャッシュを捨てる

（出典: `specs/requirements/preview-synthesis.fsl:101`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:core_upgraded#operation" digest="sha256:c291d1c326a07d08f0680420b49b473fcfa456ecbbd672cdf47ce183be0662b8" -->
#### 操作: `core_upgraded`

- 識別子: `action:core_upgraded#operation`
- 出典: `specs/requirements/preview-synthesis.fsl:102`
- パラメータ: なし

操作 `core_upgraded` を実行できるのは、次の条件を満たす場合に限る。

1. ある `p: Phrase` が存在して、`cached[p]` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. すべての `p: Phrase` について、次を適用する。

   1. `cached[p]` を `false` にする。
2. `last_step_cancelled` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-SYN-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> すべてが揃う前でも、鳴らせるフレーズだけを繋いで試唱できる

（出典: `specs/requirements/preview-synthesis.fsl:138`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:reachable:ShortenedPreview#reachability_goal" digest="sha256:3771859f430aa57ee45193e2a5da64778324a613b03d6f7852dd533f3d55a6f3" -->
#### 到達目標: `ShortenedPreview`

- 識別子: `property:reachable:ShortenedPreview#reachability_goal`
- 出典: `specs/requirements/preview-synthesis.fsl:139`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
playing and (exists p: Phrase { not resolvable[p] })
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

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:8139072967779748e6908a08b8f689a231b3933856fe7469e80ecbd4b2eca35c" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall p: Phrase { resolvable[p] and cached[p] }) and not playing and rendering == none
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `Phrase` の解析インスタンス数: 3
- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/preview-synthesis.fsl`（`KoeruPreviewSynthesis`、dialect: `requirements`）
- spec digest: `sha256:7f5a62f0b7dbd27b29a7899efbc16befa5cfb31e567207a766d8b24ec8c704e9`
- claim set digest: `sha256:238b466611199206ab20b31ba0eff62d5d7bdc41fe8344bba6d132d552698ef6`
- 形式要素の分類: rendered 25 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 4 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 4 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
