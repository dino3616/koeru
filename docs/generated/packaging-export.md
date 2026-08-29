---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/packaging-export.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:8b7e73173bd3715004a0ef9224e8de95ef0f1c03d34a0e245a76e25d38d97ea3
claim_set_digest: sha256:b120231958870dc06daf2a4529a23c0ac152619ec26cfa389f57feee5c740e5a
---

# 要件仕様書: KoeruPackagingExport

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

### AC-PKG-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 被覆できた方式を、検証して書き出し、読み戻して確かめる

（出典: `specs/requirements/packaging-export.fsl:153`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PKG-101#acceptance_trace" digest="sha256:1db039d778aed95435dbd8ad329fa72c5390ddc300e75d42992a915e0c2f13ee" -->
#### 受け入れ基準: `AC-PKG-101`

- 識別子: `acceptance:AC-PKG-101#acceptance_trace`
- 出典: `specs/requirements/packaging-export.fsl:153`
- 表題: 被覆できた方式を、検証して書き出し、読み戻して確かめる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `coverage_becomes_complete(0)`
  2. `validate()`
  3. `build_zip(0)`
  4. `verify_zip(0)`
- 期待（Then）: 最後の操作のあと、`exported[0]` が `true` である、かつ、`zip` が `Verified` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-PKG-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 破壊的な操作はスナップショットを取り、検証をやり直させる

（出典: `specs/requirements/packaging-export.fsl:161`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PKG-102#acceptance_trace" digest="sha256:1bfdd3642232d5c1272495697c93356d28c367e185af4efaded7a56db7e41f72" -->
#### 受け入れ基準: `AC-PKG-102`

- 識別子: `acceptance:AC-PKG-102#acceptance_trace`
- 出典: `specs/requirements/packaging-export.fsl:161`
- 表題: 破壊的な操作はスナップショットを取り、検証をやり直させる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `coverage_becomes_complete(0)`
  2. `validate()`
  3. `destructive_operation()`
- 期待（Then）: 最後の操作のあと、`snapshots` が `1` に等しい、かつ、`validated` が `false` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-101

**要件原文（意図。形式意味との一致は人間が確認する）**

> 被覆できていない方式は書き出せない

（出典: `specs/requirements/packaging-export.fsl:168`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-101#forbidden_trace" digest="sha256:3ce015e4fa355b4ca452a3be81fa4e835803c9ae881402016148676ebec2bfb7" -->
#### 禁止手順: `FB-PKG-101`

- 識別子: `forbidden:FB-PKG-101#forbidden_trace`
- 出典: `specs/requirements/packaging-export.fsl:168`
- 表題: 被覆できていない方式は書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `validate()`
- 期待（Then）: 続けて実行しようとする最後の操作 `build_zip(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 検証を通らないまま書き出せない

（出典: `specs/requirements/packaging-export.fsl:174`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-102#forbidden_trace" digest="sha256:a4b9da180ab86b4ba571f1862f6c31344c7a6bf9b3b711633c57b85be131f366" -->
#### 禁止手順: `FB-PKG-102`

- 識別子: `forbidden:FB-PKG-102#forbidden_trace`
- 出典: `specs/requirements/packaging-export.fsl:174`
- 表題: 検証を通らないまま書き出せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `coverage_becomes_complete(0)`
- 期待（Then）: 続けて実行しようとする最後の操作 `build_zip(0)` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PKG-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> エイリアスが衝突している間は検証を通せない

（出典: `specs/requirements/packaging-export.fsl:180`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PKG-103#forbidden_trace" digest="sha256:82b07b2d4b97ce7055868b32225091cd30d385e1226ff6fe819d45c2233ea94c" -->
#### 禁止手順: `FB-PKG-103`

- 識別子: `forbidden:FB-PKG-103#forbidden_trace`
- 出典: `specs/requirements/packaging-export.fsl:180`
- 表題: エイリアスが衝突している間は検証を通せない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `alias_collision_found()`
- 期待（Then）: 続けて実行しようとする最後の操作 `validate()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 書き出せたのは、被覆できている方式だけ

（出典: `specs/requirements/packaging-export.fsl:118`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:ExportedImpliesCovered#state_rule" digest="sha256:85ad8ae6a606a27fd68dd1898cd7eaea8a641b6eafb4afeb0554cb39200b27b4" -->
#### 状態不変条件: `ExportedImpliesCovered`

- 識別子: `property:invariant:ExportedImpliesCovered#state_rule`
- 出典: `specs/requirements/packaging-export.fsl:119`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `m: Method` について、`exported[m]` が `true` であるならば、`covered[m]` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> 読み戻し検証を通っていない ZIP は残らない

（出典: `specs/requirements/packaging-export.fsl:123`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoUnverifiedZipLeft#state_rule" digest="sha256:cb2c527117b2d9246c97fa9cb35417ac4e7107b5dc88af6c99f4aff6103b42a5" -->
#### 状態不変条件: `NoUnverifiedZipLeft`

- 識別子: `property:invariant:NoUnverifiedZipLeft#state_rule`
- 出典: `specs/requirements/packaging-export.fsl:124`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
zip == Verified => (exists m: Method { exported[m] })
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> エイリアスが衝突している間は、書き出せる状態にならない

（出典: `specs/requirements/packaging-export.fsl:128`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoValidationWithCollision#state_rule" digest="sha256:f98ae6ff0209f3b8680ba7faf416514fbd61346ff48f06dc5a22e47ab2297bc7" -->
#### 状態不変条件: `NoValidationWithCollision`

- 識別子: `property:invariant:NoValidationWithCollision#state_rule`
- 出典: `specs/requirements/packaging-export.fsl:129`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`validated` が `true` であるならば、`alias_unique` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PKG-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> 破壊的な操作の数と、取ったスナップショットの数は一致する

（出典: `specs/requirements/packaging-export.fsl:113`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:DestructiveMatchesSnapshots#state_rule" digest="sha256:49d84c295e445b041223c3e6eb1fff503620b6dd09e45c7a49591c493c4c4f52" -->
#### 状態不変条件: `DestructiveMatchesSnapshots`

- 識別子: `property:invariant:DestructiveMatchesSnapshots#state_rule`
- 出典: `specs/requirements/packaging-export.fsl:114`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`destructive_ops` が `snapshots` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### MODEL-PKG-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> ASSUME-2: スナップショットは検証用に有限へ閉じる

（出典: `specs/requirements/packaging-export.fsl:108`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:SnapshotsBounded#state_rule" digest="sha256:50ffd80d50fd1aa42b1eade204f34a6f5574e233c79540ed32088e087ccb561b" -->
#### 状態不変条件: `SnapshotsBounded`

- 識別子: `property:invariant:SnapshotsBounded#state_rule`
- 出典: `specs/requirements/packaging-export.fsl:109`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`snapshots` が `MAX_SNAPSHOTS` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-102

**要件原文（意図。形式意味との一致は人間が確認する）**

> 対象方式のエイリアス表を素材が100%被覆できたときだけ、書き出せる状態になる

（出典: `specs/requirements/packaging-export.fsl:43`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:coverage_becomes_complete#operation" digest="sha256:a24387fd29d62e7bda20b840d11637a13331fe763919f6a8784f8113279a143e" -->
#### 操作: `coverage_becomes_complete`

- 識別子: `action:coverage_becomes_complete#operation`
- 出典: `specs/requirements/packaging-export.fsl:44`
- パラメータ: `m: Method`

操作 `coverage_becomes_complete` を実行できるのは、次の条件を満たす場合に限る。

1. `covered[m]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `covered[m]` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-103

**要件原文（意図。形式意味との一致は人間が確認する）**

> エイリアスが音源全体で衝突していたら、書き出し前検証は通らない

（出典: `specs/requirements/packaging-export.fsl:49`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:alias_collision_found#operation" digest="sha256:023aee30d2b00322218cc4a78ec3771a129ee1a5ab5dd7ddbfb2bc670d3cf971" -->
#### 操作: `alias_collision_found`

- 識別子: `action:alias_collision_found#operation`
- 出典: `specs/requirements/packaging-export.fsl:50`
- パラメータ: なし

操作 `alias_collision_found` を実行できるのは、次の条件を満たす場合に限る。

1. `alias_unique` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `alias_unique` を `false` にする。
2. `validated` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-104

**要件原文（意図。形式意味との一致は人間が確認する）**

> 書き出し前に全件検査し、1件でも該当すれば書き出しを実行しない

（出典: `specs/requirements/packaging-export.fsl:56`）

> 検証で違反が見つかれば、書き出せる状態から外れる

（出典: `specs/requirements/packaging-export.fsl:63`）

> 破壊的な操作のあとは、検証をやり直す

（出典: `specs/requirements/packaging-export.fsl:138`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:validate#operation" digest="sha256:b77090a48677b538707d00313c0cf7d3c21470710daef645581e59a1a3ba710e" -->
#### 操作: `validate`

- 識別子: `action:validate#operation`
- 出典: `specs/requirements/packaging-export.fsl:57`
- パラメータ: なし

操作 `validate` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `alias_unique` が `true` である。
2. `validated` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `validated` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:validation_failed#operation" digest="sha256:00bcc79721ac60b69f266632c7c2b5a3739169966c9fbadefe9d4fb0a7addf53" -->
#### 操作: `validation_failed`

- 識別子: `action:validation_failed#operation`
- 出典: `specs/requirements/packaging-export.fsl:64`
- パラメータ: なし

操作 `validation_failed` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `validated` が `true` である。
2. `zip` が `NoZip` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `validated` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:DestructiveInvalidatesValidation#transition_rule" digest="sha256:9c6f16f0bacb63d947cfaa887c83a7198ea8b1062fae772efe5e073424c58dbb" -->
#### 遷移条件: `DestructiveInvalidatesValidation`

- 識別子: `property:trans:DestructiveInvalidatesValidation#transition_rule`
- 出典: `specs/requirements/packaging-export.fsl:139`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`destructive_ops` が 遷移前の `destructive_ops` に等しくないならば、`validated` が `false` である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-105

**要件原文（意図。形式意味との一致は人間が確認する）**

> 下位方式のパッケージは、独立した音源ルート・独立した ZIP として作る

（出典: `specs/requirements/packaging-export.fsl:93`）

> 検証を通り、被覆できている方式だけを ZIP にする

（出典: `specs/requirements/packaging-export.fsl:70`）

> 被覆できている方式を書き出せる

（出典: `specs/requirements/packaging-export.fsl:143`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:build_zip#operation" digest="sha256:8825765d16b95c06e03470e8b6be714ad12b5e104cc1c661f6797e7a963812a1" -->
#### 操作: `build_zip`

- 識別子: `action:build_zip#operation`
- 出典: `specs/requirements/packaging-export.fsl:71`
- パラメータ: `m: Method`

操作 `build_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `validated` が `true` である。
2. `covered[m]` が `true` である。
3. `exported[m]` が `false` である。
4. `zip` が `NoZip` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `zip` を `Built` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:finish_export#operation" digest="sha256:a13dac229100321e9e6a97cadff7b527345310764be16a6bafd7ef8eec33a9fd" -->
#### 操作: `finish_export`

- 識別子: `action:finish_export#operation`
- 出典: `specs/requirements/packaging-export.fsl:94`
- パラメータ: なし

操作 `finish_export` を実行できるのは、次の条件を満たす場合に限る。

1. `zip` が `Verified` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `zip` を `NoZip` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:CanExport#reachability_goal" digest="sha256:50c8bcdd56dc9b60125aef116ddc015c7b6df80b857dc03e9b6fbf103fbfb3ba" -->
#### 到達目標: `CanExport`

- 識別子: `property:reachable:CanExport#reachability_goal`
- 出典: `specs/requirements/packaging-export.fsl:144`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `m: Method` が存在して、`exported[m]` が `true` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-106

**要件原文（意図。形式意味との一致は人間が確認する）**

> 生成した ZIP を読み戻して検証する

（出典: `specs/requirements/packaging-export.fsl:79`）

> 読み戻し検証に失敗したら、書き出しを失敗として扱い ZIP を残さない

（出典: `specs/requirements/packaging-export.fsl:87`）

> 読み戻し検証に失敗して ZIP を残さない経路が存在する

（出典: `specs/requirements/packaging-export.fsl:148`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:verify_zip#operation" digest="sha256:97a81ad68fbb961f46f10645aaf9b44a9c8e2f9e3a18a202a7f354ee39e96dfc" -->
#### 操作: `verify_zip`

- 識別子: `action:verify_zip#operation`
- 出典: `specs/requirements/packaging-export.fsl:80`
- パラメータ: `m: Method`

操作 `verify_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `zip` が `Built` である。
2. `covered[m]` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `zip` を `Verified` にする。
2. `exported[m]` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:zip_verification_failed#operation" digest="sha256:267d3c0a10f17403f727861340991a7369f9053f58e275034c2cd31aabc1151d" -->
#### 操作: `zip_verification_failed`

- 識別子: `action:zip_verification_failed#operation`
- 出典: `specs/requirements/packaging-export.fsl:88`
- パラメータ: なし

操作 `zip_verification_failed` を実行できるのは、次の条件を満たす場合に限る。

1. `zip` が `Built` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `zip` を `NoZip` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ZipDiscardedOnFailure#reachability_goal" digest="sha256:4251b90f155a58387bb752ce97610c919e365f44f69c4aa39f410722ad5e5d21" -->
#### 到達目標: `ZipDiscardedOnFailure`

- 識別子: `property:reachable:ZipDiscardedOnFailure#reachability_goal`
- 出典: `specs/requirements/packaging-export.fsl:149`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
zip == NoZip and validated and (exists m: Method { covered[m] and not exported[m] })
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PKG-107

**要件原文（意図。形式意味との一致は人間が確認する）**

> 破壊的な操作の直前に自動スナップショットを取る

（出典: `specs/requirements/packaging-export.fsl:99`）

> 破壊的な操作は、必ずスナップショットを1つ増やす

（出典: `specs/requirements/packaging-export.fsl:133`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:destructive_operation#operation" digest="sha256:aa2fbf19b3dcea4ca355f5149909072b8d773fea6f0cf6c606715c67b993d58f" -->
#### 操作: `destructive_operation`

- 識別子: `action:destructive_operation#operation`
- 出典: `specs/requirements/packaging-export.fsl:100`
- パラメータ: なし

操作 `destructive_operation` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `zip` が `NoZip` である。
2. `snapshots` が `MAX_SNAPSHOTS` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `snapshots` を `snapshots + 1` にする。
2. `destructive_ops` を `destructive_ops + 1` にする。
3. `validated` を `false` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:DestructiveTakesSnapshot#transition_rule" digest="sha256:788cdc67011429054c7d4b38eff9bbb65180f2cff46036db5b209462848cbc6f" -->
#### 遷移条件: `DestructiveTakesSnapshot`

- 識別子: `property:trans:DestructiveTakesSnapshot#transition_rule`
- 出典: `specs/requirements/packaging-export.fsl:134`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

`destructive_ops` が 遷移前の `destructive_ops` に等しくないならば、`snapshots` が 遷移前の `snapshots` より大きい。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:b7b6c58a31523c624c1a93d519ba82ff8aea2cd25c7e52fa354cd11994131386" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
zip == NoZip and (forall m: Method { exported[m] })
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `Method` の解析インスタンス数: 3
- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/packaging-export.fsl`（`KoeruPackagingExport`、dialect: `requirements`）
- spec digest: `sha256:8b7e73173bd3715004a0ef9224e8de95ef0f1c03d34a0e245a76e25d38d97ea3`
- claim set digest: `sha256:b120231958870dc06daf2a4529a23c0ac152619ec26cfa389f57feee5c740e5a`
- 形式要素の分類: rendered 23 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 3 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 3 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
