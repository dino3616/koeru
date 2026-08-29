---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/telemetry-consent.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:658246348283acc33c4b69bfc0a143a95a66c6f4d99277415369a2d72b368bb8
claim_set_digest: sha256:98bdfb8aab761320479dfe729c7b187370c828f8bb8a15c6ada7bf0cdf301d38
---

# 要件仕様書: KoeruTelemetryConsent

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

### FB-TEL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 撤回したあとに送信できない

（出典: `specs/requirements/telemetry-consent.fsl:135`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-TEL-001#forbidden_trace" digest="sha256:9b70058d19010c562f862f32a7f8f00ab9d8515e36e7d78a0a6b9c897e2a55a4" -->
#### 禁止手順: `FB-TEL-001`

- 識別子: `forbidden:FB-TEL-001#forbidden_trace`
- 出典: `specs/requirements/telemetry-consent.fsl:135`
- 表題: 撤回したあとに送信できない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `create_project()`
  2. `finish_first_take()`
  3. `ask_consent()`
  4. `grant_telemetry()`
  5. `revoke_telemetry()`
- 期待（Then）: 続けて実行しようとする最後の操作 `send_event()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-TEL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 同意を求める前に送信できない

（出典: `specs/requirements/telemetry-consent.fsl:128`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-TEL-002#forbidden_trace" digest="sha256:818da1ad1bfa4bf08192e41da0e3d40d1d1cef46d30977f656bf995438b1dc0a" -->
#### 禁止手順: `FB-TEL-002`

- 識別子: `forbidden:FB-TEL-002#forbidden_trace`
- 出典: `specs/requirements/telemetry-consent.fsl:128`
- 表題: 同意を求める前に送信できない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `create_project()`
  2. `finish_first_take()`
- 期待（Then）: 続けて実行しようとする最後の操作 `send_event()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-TEL-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 計測に同意しただけでは、クラッシュレポートは送られない

（出典: `specs/requirements/telemetry-consent.fsl:145`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-TEL-003#forbidden_trace" digest="sha256:bce52403f8086f18e19d20b7dac1af09d3c59ec6653c5b41c769a1fa4cb8b70a" -->
#### 禁止手順: `FB-TEL-003`

- 識別子: `forbidden:FB-TEL-003#forbidden_trace`
- 出典: `specs/requirements/telemetry-consent.fsl:145`
- 表題: 計測に同意しただけでは、クラッシュレポートは送られない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `create_project()`
  2. `finish_first_take()`
  3. `ask_consent()`
  4. `grant_telemetry()`
- 期待（Then）: 続けて実行しようとする最後の操作 `send_crash_report()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-TEL-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 最初の録音を終える前に同意を求められない

（出典: `specs/requirements/telemetry-consent.fsl:154`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-TEL-004#forbidden_trace" digest="sha256:d69a57c4ee7170f21152686601bca6f72dc5b2ae0da8b1d4050347aa94b11de9" -->
#### 禁止手順: `FB-TEL-004`

- 識別子: `forbidden:FB-TEL-004#forbidden_trace`
- 出典: `specs/requirements/telemetry-consent.fsl:154`
- 表題: 最初の録音を終える前に同意を求められない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `create_project()`
- 期待（Then）: 続けて実行しようとする最後の操作 `ask_consent()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-TEL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 訊いていないうちは、どちらの送信も起きない

（出典: `specs/requirements/telemetry-consent.fsl:107`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NothingSentBeforeAsking#state_rule" digest="sha256:cf5d941cf2ba5eaf4a31f5b76dd823819a0f5ec040d5cf6b754b493c9ecd9010" -->
#### 状態不変条件: `NothingSentBeforeAsking`

- 識別子: `property:invariant:NothingSentBeforeAsking#state_rule`
- 出典: `specs/requirements/telemetry-consent.fsl:108`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

（`sent_events` が `0` より大きい、または、`sent_crashes` が `0` より大きい）ならば、`asked` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-TEL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 送信が起きるのは、その種別の同意がある間だけ

（出典: `specs/requirements/telemetry-consent.fsl:112`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:trans:SendOnlyWhileConsented#transition_rule" digest="sha256:be30b5a7899f95a2af77452e8c7297ad07140f4c2791ed384e7b22d2ecfacd69" -->
#### 遷移条件: `SendOnlyWhileConsented`

- 識別子: `property:trans:SendOnlyWhileConsented#transition_rule`
- 出典: `specs/requirements/telemetry-consent.fsl:113`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(sent_events != old(sent_events) => telemetry == Granted) and (sent_crashes != old(sent_crashes) => crash_report == Granted)
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-TEL-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 同意の状態が動いているなら、必ず訊いたあと

（出典: `specs/requirements/telemetry-consent.fsl:102`）

> 同意を求める前に、プロジェクト作成と最初の録音が終わっている

（出典: `specs/requirements/telemetry-consent.fsl:97`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:AskedOnlyAfterFirstUse#state_rule" digest="sha256:eed89285155b693784a9a73ae7e453429805459c8131cd7c27fa32de571455ec" -->
#### 状態不変条件: `AskedOnlyAfterFirstUse`

- 識別子: `property:invariant:AskedOnlyAfterFirstUse#state_rule`
- 出典: `specs/requirements/telemetry-consent.fsl:98`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`asked` が `true` であるならば、（`project_created` が `true` である、かつ、`first_take_done` が `true` である）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:ConsentImpliesAsked#state_rule" digest="sha256:de605bb9fa98939da65bf927a6c7168f411c378126fb4365780994857e12bab7" -->
#### 状態不変条件: `ConsentImpliesAsked`

- 識別子: `property:invariant:ConsentImpliesAsked#state_rule`
- 出典: `specs/requirements/telemetry-consent.fsl:103`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

（`telemetry` が `NotAsked` でない、または、`crash_report` が `NotAsked` でない）ならば、`asked` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 利用計測は既定オフのオプトイン。訊いていない間は同意を得られない

（出典: `specs/requirements/telemetry-consent.fsl:57`）

> 同意を得たうえで送信が起きる経路が存在する

（出典: `specs/requirements/telemetry-consent.fsl:118`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:grant_telemetry#operation" digest="sha256:24c5a3cd154b92015a0bf1a3be23f2988beca02cb1af9907a8252a8a47869e5b" -->
#### 操作: `grant_telemetry`

- 識別子: `action:grant_telemetry#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:58`
- パラメータ: なし

操作 `grant_telemetry` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `asked` が `true` である。
2. `telemetry` が `Granted` でない。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `telemetry` を `Granted` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:SentAfterConsent#reachability_goal" digest="sha256:e9decff3908fa1c4920c9a627f8d33a1ecbefefb3702bb321f71f63f13b81e60" -->
#### 到達目標: `SentAfterConsent`

- 識別子: `property:reachable:SentAfterConsent#reachability_goal`
- 出典: `specs/requirements/telemetry-consent.fsl:119`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`sent_events` が `0` より大きい、かつ、`telemetry` が `Granted` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 同意はいつでも撤回できる

（出典: `specs/requirements/telemetry-consent.fsl:64`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:revoke_telemetry#operation" digest="sha256:1ef3003cc4542db452a5cf09bca18d6a13d80a135bcad6cacae94a0b4ee9efb2" -->
#### 操作: `revoke_telemetry`

- 識別子: `action:revoke_telemetry#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:65`
- パラメータ: なし

操作 `revoke_telemetry` を実行できるのは、次の条件を満たす場合に限る。

1. `telemetry` が `Granted` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `telemetry` を `Revoked` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 送信は同意がある間だけ行える

（出典: `specs/requirements/telemetry-consent.fsl:83`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:send_event#operation" digest="sha256:e42fde75092151fa2a3301114f6bf2000cf3352a9fcd5df10ba52d549fe23663" -->
#### 操作: `send_event`

- 識別子: `action:send_event#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:84`
- パラメータ: なし

操作 `send_event` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `telemetry` が `Granted` である。
2. `sent_events` が `2` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `sent_events` を `sent_events + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> プロジェクトを作る前に同意を求めない

（出典: `specs/requirements/telemetry-consent.fsl:36`）

> 同意を一度も求めないまま、録音を終えて使い続けられる

（出典: `specs/requirements/telemetry-consent.fsl:123`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:create_project#operation" digest="sha256:9be374211d2573b5a86f8a3f2eefb5e819a2c331b30086756a308cf9ff5050d6" -->
#### 操作: `create_project`

- 識別子: `action:create_project#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:37`
- パラメータ: なし

操作 `create_project` を実行できるのは、次の条件を満たす場合に限る。

1. `project_created` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `project_created` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:UsableWithoutAsking#reachability_goal" digest="sha256:989f381477b2ae1141a22fd64f200df322309429d3f1dcefd78c4856581284f5" -->
#### 到達目標: `UsableWithoutAsking`

- 識別子: `property:reachable:UsableWithoutAsking#reachability_goal`
- 出典: `specs/requirements/telemetry-consent.fsl:124`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`first_take_done` が `true` である、かつ、`asked` が `false` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-005

**要件原文（意図。形式意味との一致は人間が確認する）**

> 最初の録音を終える前に同意を求めない

（出典: `specs/requirements/telemetry-consent.fsl:42`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:finish_first_take#operation" digest="sha256:bafc20a41036523152e10f90f7a9166bb4a984b1f140d6defec970bb8c5c5397" -->
#### 操作: `finish_first_take`

- 識別子: `action:finish_first_take#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:43`
- パラメータ: なし

操作 `finish_first_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `project_created` が `true` である。
2. `first_take_done` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `first_take_done` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-006

**要件原文（意図。形式意味との一致は人間が確認する）**

> 同意を求めるのは、プロジェクト作成と最初の録音を終えたあと。一度だけ

（出典: `specs/requirements/telemetry-consent.fsl:49`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:ask_consent#operation" digest="sha256:6a8fd007b0a006538da80d12118e6590ba2cccdf7ce9b335f81a5789b1c40d7b" -->
#### 操作: `ask_consent`

- 識別子: `action:ask_consent#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:50`
- パラメータ: なし

操作 `ask_consent` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `asked` が `false` である。
2. `project_created` が `true` である。
3. `first_take_done` が `true` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `asked` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-TEL-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> クラッシュレポートの同意も独立に撤回できる

（出典: `specs/requirements/telemetry-consent.fsl:77`）

> クラッシュレポートの送信は、その同意がある間だけ行える

（出典: `specs/requirements/telemetry-consent.fsl:90`）

> クラッシュレポートは計測と別の同意単位にする

（出典: `specs/requirements/telemetry-consent.fsl:70`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:grant_crash_report#operation" digest="sha256:c71065de7b6d39545f5d222e06173c5adbb4aebec11f5a9ceb96e19f4fc0e75a" -->
#### 操作: `grant_crash_report`

- 識別子: `action:grant_crash_report#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:71`
- パラメータ: なし

操作 `grant_crash_report` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `asked` が `true` である。
2. `crash_report` が `Granted` でない。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `crash_report` を `Granted` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:revoke_crash_report#operation" digest="sha256:e78e1a77c9334d99f85b1532c0731358e8a4aa2d67330d70797f6a9d978a5b6e" -->
#### 操作: `revoke_crash_report`

- 識別子: `action:revoke_crash_report#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:78`
- パラメータ: なし

操作 `revoke_crash_report` を実行できるのは、次の条件を満たす場合に限る。

1. `crash_report` が `Granted` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `crash_report` を `Revoked` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:send_crash_report#operation" digest="sha256:e460865525fb3a66afbb4396552845fb1d16ca1b43196828f487627180999147" -->
#### 操作: `send_crash_report`

- 識別子: `action:send_crash_report#operation`
- 出典: `specs/requirements/telemetry-consent.fsl:91`
- パラメータ: なし

操作 `send_crash_report` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `crash_report` が `Granted` である。
2. `sent_crashes` が `2` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `sent_crashes` を `sent_crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:edd73382ebacaf435a189cbf604cb72194db3e5d469a52db6b36c60fd873674b" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次のすべてが成立する。

1. `asked` が `true` である。

2. `telemetry` が `NotAsked` でない。

3. `crash_report` が `NotAsked` でない。

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/telemetry-consent.fsl`（`KoeruTelemetryConsent`、dialect: `requirements`）
- spec digest: `sha256:658246348283acc33c4b69bfc0a143a95a66c6f4d99277415369a2d72b368bb8`
- claim set digest: `sha256:98bdfb8aab761320479dfe729c7b187370c828f8bb8a15c6ada7bf0cdf301d38`
- 形式要素の分類: rendered 19 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 1 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 1 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
