---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/requirements/song-coverage.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:0bcc594b2f8c9b477b8e1b8c15fc684581bce0246454f5d5802e28bd7895811e
claim_set_digest: sha256:5993d23a199dcdc685687a6f1ef3c502ae64ccfe4daf92b4d82fdd04d7da2b51
---

# 要件仕様書: KoeruSongCoverage

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

### AC-RCL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 低域だけ録ると、低域しか要らない曲だけが完全になる

（出典: `specs/requirements/song-coverage.fsl:113`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-RCL-001#acceptance_trace" digest="sha256:7b7a1f7694d3715885ee48c8daff36e41e0f96d82afb7a6e1fc672dcfd225279" -->
#### 受け入れ基準: `AC-RCL-001`

- 識別子: `acceptance:AC-RCL-001#acceptance_trace`
- 出典: `specs/requirements/song-coverage.fsl:113`
- 表題: 低域だけ録ると、低域しか要らない曲だけが完全になる

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `song_requires_high(1)`
  2. `pitch_becomes_covered(0)`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  (song_needs_low[0] => pitch_covered[0]) and (song_needs_high[0] => pitch_covered[1]) and not ((song_needs_low[1] => pitch_covered[0]) and (song_needs_high[1] => pitch_covered[1])) and not fallback_ok[1]
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### AC-RCL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 採用テイクを切り替えても、歌える曲の数は変わらない

（出典: `specs/requirements/song-coverage.fsl:120`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="acceptance:AC-RCL-002#acceptance_trace" digest="sha256:6f98464104446d5eda904bbda3af52f11ee1167051b7a4d21525d33c4ee831c9" -->
#### 受け入れ基準: `AC-RCL-002`

- 識別子: `acceptance:AC-RCL-002#acceptance_trace`
- 出典: `specs/requirements/song-coverage.fsl:120`
- 表題: 採用テイクを切り替えても、歌える曲の数は変わらない

この受け入れ基準は、一つの具体的な実行例である。

- 前提（Given）: 初期化直後の状態から開始する。
- 操作（When）: 次の操作をこの順に実行する。いずれも拒否されずに成功しなければならない。
  1. `pitch_becomes_covered(0)`
  2. `swap_adopted_take()`
- 期待（Then）: 最後の操作のあと、次が成立する。

  ```fsl
  (song_needs_low[0] => pitch_covered[0]) and (song_needs_high[0] => pitch_covered[1]) and ((song_needs_low[1] => pitch_covered[0]) and (song_needs_high[1] => pitch_covered[1]))
  ```

この基準が示すのは、上記の一連の操作が成功し、期待が成立することのみである。同種のすべての入力・順序・状態で同じ結果になることを主張するものではない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-RCL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> 必要な音高が揃っている曲は「完全」になる

（出典: `specs/requirements/song-coverage.fsl:79`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:CompleteMeansAllPitchesCovered#state_rule" digest="sha256:57adea97ee2f5765588c308104eaaabe4fb30832f0659f5f7dc82b02c5eb897a" -->
#### 状態不変条件: `CompleteMeansAllPitchesCovered`

- 識別子: `property:invariant:CompleteMeansAllPitchesCovered#state_rule`
- 出典: `specs/requirements/song-coverage.fsl:80`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall s: Song { ((song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1])) == ((song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1])) }
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-RCL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 1音高だけ録り終えても、音域の広い曲は完全にならない

（出典: `specs/requirements/song-coverage.fsl:84`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:WideSongNeedsBothPitches#state_rule" digest="sha256:9c68eda9dd5de7575156aa26e7f126ff1a39291509eb4dde017e60b41901b997" -->
#### 状態不変条件: `WideSongNeedsBothPitches`

- 識別子: `property:invariant:WideSongNeedsBothPitches#state_rule`
- 出典: `specs/requirements/song-coverage.fsl:85`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall s: Song { song_needs_high[s] and not pitch_covered[1] => not ((song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1])) }
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-RCL-001

**要件原文（意図。形式意味との一致は人間が確認する）**

> その音高の必要単位が揃うと、その音高は収録済みになる

（出典: `specs/requirements/song-coverage.fsl:52`）

> 収録済みになった音高は、そのあと未収録に戻らない

（出典: `specs/requirements/song-coverage.fsl:96`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:pitch_becomes_covered#operation" digest="sha256:77f303944a265721fab8d2d2e24338b10ad237d622183ce5ccca5ba9f76950e8" -->
#### 操作: `pitch_becomes_covered`

- 識別子: `action:pitch_becomes_covered#operation`
- 出典: `specs/requirements/song-coverage.fsl:53`
- パラメータ: `p: Pitch`

操作 `pitch_becomes_covered` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `pitch_covered[p]` が `false` である。
2. `takes` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `pitch_covered[p]` を `true` にする。
2. `takes` を `takes + 1` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:CoverageIsMonotone#transition_rule" digest="sha256:13be2970fb54c117055001b1a109e81d17499c37596f997a2028a7a690cbfbbe" -->
#### 遷移条件: `CoverageIsMonotone`

- 識別子: `property:trans:CoverageIsMonotone#transition_rule`
- 出典: `specs/requirements/song-coverage.fsl:97`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall p: Pitch { old(pitch_covered[p]) => pitch_covered[p] }
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-RCL-002

**要件原文（意図。形式意味との一致は人間が確認する）**

> 低域だけ録った状態で、狭い曲は歌えて広い曲は歌えない

（出典: `specs/requirements/song-coverage.fsl:106`）

> 音域の広い曲は、複数の収録音高を要求する

（出典: `specs/requirements/song-coverage.fsl:60`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:song_requires_high#operation" digest="sha256:5902dea1d872fe12ad356d51f1fd893b0d5050e6be52f1df97c1f24555e67bdc" -->
#### 操作: `song_requires_high`

- 識別子: `action:song_requires_high#operation`
- 出典: `specs/requirements/song-coverage.fsl:61`
- パラメータ: `s: Song`

操作 `song_requires_high` を実行できるのは、次の条件を満たす場合に限る。

1. `song_needs_high[s]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `song_needs_high[s]` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:NarrowSingableWideNot#reachability_goal" digest="sha256:64183f26264a75894a69b2d727b56ddd2e43afec7cfbc1a40a5a8e4e5053f46e" -->
#### 到達目標: `NarrowSingableWideNot`

- 識別子: `property:reachable:NarrowSingableWideNot#reachability_goal`
- 出典: `specs/requirements/song-coverage.fsl:107`

次の状態に到達する実行例が存在しなければならない（到達目標）。

ある `a: Song` が存在して、次が成立する。

```fsl
exists b: Song { a != b and ((song_needs_low[a] => pitch_covered[0]) and (song_needs_high[a] => pitch_covered[1])) and (not ((song_needs_low[b] => pitch_covered[0]) and (song_needs_high[b] => pitch_covered[1])) and not fallback_ok[b]) }
```。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-RCL-003

**要件原文（意図。形式意味との一致は人間が確認する）**

> 一部が未収録でも、フォールバックで全ノートが鳴るなら歌える

（出典: `specs/requirements/song-coverage.fsl:66`）

> 代替ありで歌える状態が生じうる

（出典: `specs/requirements/song-coverage.fsl:101`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:fallback_resolves#operation" digest="sha256:6f41f0aea4e11760f39c4d2f1841b3fe3f4d3bc1f69b1fe7c9af5d61cf47736b" -->
#### 操作: `fallback_resolves`

- 識別子: `action:fallback_resolves#operation`
- 出典: `specs/requirements/song-coverage.fsl:67`
- パラメータ: `s: Song`

操作 `fallback_resolves` を実行できるのは、次の条件を満たす場合に限る。

1. `fallback_ok[s]` が `false` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `fallback_ok[s]` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:SingableWithFallback#reachability_goal" digest="sha256:7301b94dbb6b9d3c957bedc22dfc672ea572f1d480c1142c7eef962896e6f593" -->
#### 到達目標: `SingableWithFallback`

- 識別子: `property:reachable:SingableWithFallback#reachability_goal`
- 出典: `specs/requirements/song-coverage.fsl:102`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
exists s: Song { not ((song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1])) and fallback_ok[s] }
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-RCL-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> 採用テイクの切り替えは、その行が生む収録単位の集合を変えない

（出典: `specs/requirements/song-coverage.fsl:72`）

> 採用テイクを切り替えても、歌える曲の数は変わらない

（出典: `specs/requirements/song-coverage.fsl:91`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:swap_adopted_take#operation" digest="sha256:af95dd2d2cbce44aa14af78a50b603971b743bab45811860a9088f72de0f04ca" -->
#### 操作: `swap_adopted_take`

- 識別子: `action:swap_adopted_take#operation`
- 出典: `specs/requirements/song-coverage.fsl:73`
- パラメータ: なし

操作 `swap_adopted_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `takes` が `0` より大きい。
2. `adopted_swaps` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `adopted_swaps` を `adopted_swaps + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:SwapDoesNotChangeCoverage#transition_rule" digest="sha256:b6ff09185049b032fcee557de8dc3901bedde0842a3cb131c6815f6f10c6d17d" -->
#### 遷移条件: `SwapDoesNotChangeCoverage`

- 識別子: `property:trans:SwapDoesNotChangeCoverage#transition_rule`
- 出典: `specs/requirements/song-coverage.fsl:92`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
adopted_swaps != old(adopted_swaps) => count(s: Song where (song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1]) or fallback_ok[s]) == old(count(s: Song where (song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1]) or fallback_ok[s]))
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="terminal#terminal_rule" digest="sha256:53c957ab60e79aa9b7f21114bb4411648838a45cf681fca58959ce66dcd0779d" -->
#### 終端条件

- 識別子: `terminal#terminal_rule`

次の条件を満たす状態は、意図された終端状態（terminal）である。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall s: Song { (song_needs_low[s] => pitch_covered[0]) and (song_needs_high[s] => pitch_covered[1]) }
```

終端状態では、それ以上操作を進めないことが意図されている。これは到達を要求する条件ではない。デッドロック検査において「意図された停止」を「意図しない停止」から区別するための宣言である。
<!-- fsl:claim end -->

これらの要素は要件 ID を持たないため、外部エビデンスを対応付けられない。保証クラスは一律 `not_run` である。

## 未決定事項

未決定として宣言された事項はない。

## 解析スコープ

検証は次の範囲で行われる。これは解析のための範囲であり、実運用上の上限や容量を意味しない。

- エンティティ `Song` の解析インスタンス数: 2
- エンティティ `Pitch` の解析インスタンス数: 2
- 数値 `Count` の解析値域: `0` から `3` まで

## 生成情報

- 生成元仕様: `specs/requirements/song-coverage.fsl`（`KoeruSongCoverage`、dialect: `requirements`）
- spec digest: `sha256:0bcc594b2f8c9b477b8e1b8c15fc684581bce0246454f5d5802e28bd7895811e`
- claim set digest: `sha256:5993d23a199dcdc685687a6f1ef3c502ae64ccfe4daf92b4d82fdd04d7da2b51`
- 形式要素の分類: rendered 12 件 / unattributed 1 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 8 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 8 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
