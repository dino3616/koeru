---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/first-run.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:16c418cabdf68eea1bbe3d7b5d32b0611a0ba4dd15bd6aacb5b696cb84e592a3
claim_set_digest: sha256:3c0d8ee163514393f7b290447e738fc4b209024699d4617af1c2e2d02fc4d2cc
---

# 要件仕様書: KoeruFirstRun

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

### AC-PLT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 起動・名前・方式・マイクの4つで最初のフレーズに着く

（出典: `specs/requirements/first-run.fsl:155`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PLT-001#acceptance_trace" digest="sha256:1e14d3b51fca12b937597bf1184772b1e67478e384ead93d504570e54d3dc37b" -->
#### 受け入れ基準: `AC-PLT-001`

- 識別子: `acceptance:AC-PLT-001#acceptance_trace`
- 出典: `specs/requirements/first-run.fsl:155`
- 表題: 起動・名前・方式・マイクの4つで最初のフレーズに着く

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `launch()`
  2. `enter_name()`
  3. `choose_method()`
  4. `request_mic()`
  5. `show_first_phrase()`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  phrase_shown and not wizard_shown and not terms_accepted
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-PLT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 拒否されても、許可し直せば再起動なしで録音できる

（出典: `specs/requirements/first-run.fsl:164`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-PLT-002#acceptance_trace" digest="sha256:a465227db040739d86b9ce7d4a301b380eb48188da41a6ef12ba0d3eb3a27946" -->
#### 受け入れ基準: `AC-PLT-002`

- 識別子: `acceptance:AC-PLT-002#acceptance_trace`
- 出典: `specs/requirements/first-run.fsl:164`
- 表題: 拒否されても、許可し直せば再起動なしで録音できる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `launch()`
  2. `enter_name()`
  3. `choose_method()`
  4. `deny_mic()`
  5. `grant_after_denial()`
  6. `show_first_phrase()`
  7. `start_recording()`
- 期待（Then）: 最後の操作のあと、`recording` が `true` である。

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PLT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 起動時にマイク権限を求めない

（出典: `specs/requirements/first-run.fsl:175`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PLT-001#forbidden_trace" digest="sha256:ec2079495fb0f0b42fbc48c66ebf0dea9ef76b5abdd6e17b0f31c755284b8c2b" -->
#### 禁止手順: `FB-PLT-001`

- 識別子: `forbidden:FB-PLT-001#forbidden_trace`
- 出典: `specs/requirements/first-run.fsl:175`
- 表題: 起動時にマイク権限を求めない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `launch()`
- 期待（Then）: 続けて実行しようとする最後の操作 `request_mic()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PLT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 最初のフレーズより前に利用規約へ同意させない

（出典: `specs/requirements/first-run.fsl:181`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PLT-002#forbidden_trace" digest="sha256:adf51aec8f5d897a7079c01c71cf12c5419dddc0a3dccb423881136bf6c9d6b1" -->
#### 禁止手順: `FB-PLT-002`

- 識別子: `forbidden:FB-PLT-002#forbidden_trace`
- 出典: `specs/requirements/first-run.fsl:181`
- 表題: 最初のフレーズより前に利用規約へ同意させない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `launch()`
  2. `enter_name()`
- 期待（Then）: 続けて実行しようとする最後の操作 `accept_terms()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### FB-PLT-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> マイクが拒否されたまま録音を始めない

（出典: `specs/requirements/first-run.fsl:188`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="forbidden:FB-PLT-003#forbidden_trace" digest="sha256:e2044d94eadea7b65bafb015b128cc8a932864523e47cfbab01b7c9f49d6b03b" -->
#### 禁止手順: `FB-PLT-003`

- 識別子: `forbidden:FB-PLT-003#forbidden_trace`
- 出典: `specs/requirements/first-run.fsl:188`
- 表題: マイクが拒否されたまま録音を始めない

この禁止手順は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 先行手順（When）: 次の操作をこの順に実行する。いずれも成功しなければならない。
  1. `launch()`
  2. `enter_name()`
  3. `choose_method()`
  4. `deny_mic()`
- 期待（Then）: 続けて実行しようとする最後の操作 `show_first_phrase()` は、拒否されなければならない（この時点では実行できてはならない）。

この基準が示すのは、上記の手順の直後に最後の操作が拒否されることのみである。この操作があらゆる状況で禁止されることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PLT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 最初のフレーズが出るまでに、4つ以外の関門は1つも挟まらない

（出典: `specs/requirements/first-run.fsl:128`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NothingBeforeFirstPhrase#state_rule" digest="sha256:d3a827a389c889882d2f5efe31aab83d90c66679ca07914f98eafed40f07f351" -->
#### 状態不変条件: `NothingBeforeFirstPhrase`

- 識別子: `property:invariant:NothingBeforeFirstPhrase#state_rule`
- 出典: `specs/requirements/first-run.fsl:129`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
not phrase_shown => not account_created and not signed_in and not terms_accepted and not wizard_shown
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PLT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> マイクが使えないまま、無音で録音が進むことはない

（出典: `specs/requirements/first-run.fsl:134`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoSilentRecording#state_rule" digest="sha256:5b98d7c730b55590571ca714600b2bab10a4f06acaa108ba244d255bd0951163" -->
#### 状態不変条件: `NoSilentRecording`

- 識別子: `property:invariant:NoSilentRecording#state_rule`
- 出典: `specs/requirements/first-run.fsl:135`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`recording` が `true` であるならば、`mic` が `Granted` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-PLT-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> マイク権限を求めるのは、方式を選んだあと

（出典: `specs/requirements/first-run.fsl:139`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:MicAskedAfterMethod#state_rule" digest="sha256:9512da2beb8a3557bafce952a5f8a3fb874e9134473c844c6c9177c6689042b4" -->
#### 状態不変条件: `MicAskedAfterMethod`

- 識別子: `property:invariant:MicAskedAfterMethod#state_rule`
- 出典: `specs/requirements/first-run.fsl:140`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`mic` が `NotRequested` でないならば、`method_chosen` が `true` である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PLT-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 1つめの操作は起動

（出典: `specs/requirements/first-run.fsl:43`）

> 2つめの操作は音源名の入力。既定値のまま通過できる

（出典: `specs/requirements/first-run.fsl:49`）

> 3つめの操作は方式の選択

（出典: `specs/requirements/first-run.fsl:56`）

> 4つの操作だけで最初のフレーズに到達できる

（出典: `specs/requirements/first-run.fsl:149`）

> 4つの操作を終えると最初のフレーズが出る

（出典: `specs/requirements/first-run.fsl:83`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:choose_method#operation" digest="sha256:34c9732b30c1d24973145f4bea6937cc52bbfb8cbe05c2e71ee4e81d0860248a" -->
#### 操作: `choose_method`

- 識別子: `action:choose_method#operation`
- 出典: `specs/requirements/first-run.fsl:57`
- パラメータ: なし

操作 `choose_method` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `name_entered` が `true` である。
2. `method_chosen` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `method_chosen` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:enter_name#operation" digest="sha256:faddce8743a0bb85625f34f15cd0a497995b959024d021c4576bf7e680ef69fa" -->
#### 操作: `enter_name`

- 識別子: `action:enter_name#operation`
- 出典: `specs/requirements/first-run.fsl:50`
- パラメータ: なし

操作 `enter_name` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `launched` が `true` である。
2. `name_entered` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `name_entered` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:launch#operation" digest="sha256:933215d7d4600f5a45988f23a03eea157b266dbb98ad9a3ee5847a19ec72cb41" -->
#### 操作: `launch`

- 識別子: `action:launch#operation`
- 出典: `specs/requirements/first-run.fsl:44`
- パラメータ: なし

操作 `launch` を実行できるのは、次の条件を満たす場合に限る。

1. `launched` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `launched` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:show_first_phrase#operation" digest="sha256:f2feaf26c6e5594d1894d5fe744c9047ec5dd5d20d545216dd8f0a2f7d197234" -->
#### 操作: `show_first_phrase`

- 識別子: `action:show_first_phrase#operation`
- 出典: `specs/requirements/first-run.fsl:84`
- パラメータ: なし

操作 `show_first_phrase` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `method_chosen` が `true` である。
2. `mic` が `Granted` である。
3. `phrase_shown` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `phrase_shown` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:ReachesFirstPhraseCleanly#reachability_goal" digest="sha256:fd1895fa2ba2cf312c92df4a743873a91ce8e38210965154ab464962869cb9e9" -->
#### 到達目標: `ReachesFirstPhraseCleanly`

- 識別子: `property:reachable:ReachesFirstPhraseCleanly#reachability_goal`
- 出典: `specs/requirements/first-run.fsl:150`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次のすべてが成立する。

1. `phrase_shown` が `true` である。

2. `account_created` が `false` である。

3. `signed_in` が `false` である。

4. `terms_accepted` が `false` である。

5. `wizard_shown` が `false` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PLT-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> マイク権限は初回の録音画面に入る直前に1回だけ求める

（出典: `specs/requirements/first-run.fsl:63`）

> 拒否されたあと、再起動せずに録音まで到達できる

（出典: `specs/requirements/first-run.fsl:144`）

> 拒否されることもある

（出典: `specs/requirements/first-run.fsl:70`）

> 権限が付与されたら、再起動せずに録音へ戻れる

（出典: `specs/requirements/first-run.fsl:77`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:deny_mic#operation" digest="sha256:4331923a61434ae67da30d50bca7ec8a789d7866473263cc849509a9f202ccbf" -->
#### 操作: `deny_mic`

- 識別子: `action:deny_mic#operation`
- 出典: `specs/requirements/first-run.fsl:71`
- パラメータ: なし

操作 `deny_mic` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `method_chosen` が `true` である。
2. `mic` が `NotRequested` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `mic` を `Denied` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:grant_after_denial#operation" digest="sha256:2e0b847e1ab8e0e5f2951994d6d5604a0a5b10bf5e9f2650ec940d6b20f31d66" -->
#### 操作: `grant_after_denial`

- 識別子: `action:grant_after_denial#operation`
- 出典: `specs/requirements/first-run.fsl:78`
- パラメータ: なし

操作 `grant_after_denial` を実行できるのは、次の条件を満たす場合に限る。

1. `mic` が `Denied` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `mic` を `Granted` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:request_mic#operation" digest="sha256:b5972d286dd8c2f973eaaa5ddaad0ce5a964e2f6ea7357037cf1702e3778741c" -->
#### 操作: `request_mic`

- 識別子: `action:request_mic#operation`
- 出典: `specs/requirements/first-run.fsl:64`
- パラメータ: なし

操作 `request_mic` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `method_chosen` が `true` である。
2. `mic` が `NotRequested` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `mic` を `Granted` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:RecoversFromDenial#reachability_goal" digest="sha256:82186c1e760705f048f33fc00f2ef07b6a24467a8c856029c5973eebd1d7191f" -->
#### 到達目標: `RecoversFromDenial`

- 識別子: `property:reachable:RecoversFromDenial#reachability_goal`
- 出典: `specs/requirements/first-run.fsl:145`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`recording` が `true` である、かつ、`mic` が `Granted` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PLT-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> マイクが使えるときだけ録音を始める

（出典: `specs/requirements/first-run.fsl:91`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:start_recording#operation" digest="sha256:18358322a153bed2a0ccfd832008012dbd011b06532730f34861c0d176ea1439" -->
#### 操作: `start_recording`

- 識別子: `action:start_recording#operation`
- 出典: `specs/requirements/first-run.fsl:92`
- パラメータ: なし

操作 `start_recording` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `phrase_shown` が `true` である。
2. `mic` が `Granted` である。
3. `recording` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `true` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-PLT-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> アカウント作成は、最初のフレーズより前に挟まない

（出典: `specs/requirements/first-run.fsl:100`）

> サインインは、最初のフレーズより前に挟まない

（出典: `specs/requirements/first-run.fsl:107`）

> 初回設定ウィザードは、最初のフレーズより前に挟まない

（出典: `specs/requirements/first-run.fsl:121`）

> 利用規約への同意は、最初のフレーズより前に挟まない

（出典: `specs/requirements/first-run.fsl:114`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:accept_terms#operation" digest="sha256:b9f4cdffc2251bd5a72f91ae10ab7d0e955636492b6909d8d76cd2ff1cc33f0d" -->
#### 操作: `accept_terms`

- 識別子: `action:accept_terms#operation`
- 出典: `specs/requirements/first-run.fsl:115`
- パラメータ: なし

操作 `accept_terms` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `phrase_shown` が `true` である。
2. `terms_accepted` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `terms_accepted` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:create_account#operation" digest="sha256:47721a554b23e6643ae6068c2ed38173d25fac464cf3f6aa0c4f87af64cc3de3" -->
#### 操作: `create_account`

- 識別子: `action:create_account#operation`
- 出典: `specs/requirements/first-run.fsl:101`
- パラメータ: なし

操作 `create_account` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `phrase_shown` が `true` である。
2. `account_created` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `account_created` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:show_wizard#operation" digest="sha256:728c5d574e48dddad332c91951edcf40ddc905f2212d7e92523020059d4bbf7d" -->
#### 操作: `show_wizard`

- 識別子: `action:show_wizard#operation`
- 出典: `specs/requirements/first-run.fsl:122`
- パラメータ: なし

操作 `show_wizard` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `phrase_shown` が `true` である。
2. `wizard_shown` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `wizard_shown` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:sign_in#operation" digest="sha256:02a252970aedf7b9c0d73500608107c4eb6f76fd9956b37d1132650ab5c83feb" -->
#### 操作: `sign_in`

- 識別子: `action:sign_in#operation`
- 出典: `specs/requirements/first-run.fsl:108`
- パラメータ: なし

操作 `sign_in` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `phrase_shown` が `true` である。
2. `signed_in` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `signed_in` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:af5963f37fbc97a561734f967b01ca93c136b6a7878ada192e166182a59feac6" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

`recording` が `true` である。

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

本仕様に解析スコープの宣言（instances / values）はない。

## 生成情報

- 生成元仕様: `specs/requirements/first-run.fsl`（`KoeruFirstRun`、dialect: `requirements`）
- spec digest: `sha256:16c418cabdf68eea1bbe3d7b5d32b0611a0ba4dd15bd6aacb5b696cb84e592a3`
- claim set digest: `sha256:3c0d8ee163514393f7b290447e738fc4b209024699d4617af1c2e2d02fc4d2cc`
- 形式要素の分類: rendered 22 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 2 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 2 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
