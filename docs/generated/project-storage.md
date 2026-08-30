---
fsl_document_schema: fsl-requirements-document-v2
view: requirements
lang: ja
source: specs/design/project-storage.fsl
renderer: fslc-document-renderer
renderer_version: 1.3.0
normative_scope: generated-claim-blocks-only
spec_digest: sha256:1606c38c2884202c3531da7dc6db999dfea16f6ad09b7746708f66dbc170c072
claim_set_digest: sha256:9cf110ac21bb11e246a9cbef6236b77c0758f6a203d6c7084e9ae782bb658b84
---

# 要件仕様書: KoeruProjectStorage

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

### INV-REC-201

**要件原文（意図。形式意味との一致は人間が確認する）**

> ファイルの無いテイク行が DB にできない

（出典: `specs/design/project-storage.fsl:164`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:NoOrphanTakeRow#state_rule" digest="sha256:df73ab8d64b185dfe0e919969db11b9bfe852b1a1b2f4bcdd1ff095f7e321556" -->
#### 状態不変条件: `NoOrphanTakeRow`

- 識別子: `property:invariant:NoOrphanTakeRow#state_rule`
- 出典: `specs/design/project-storage.fsl:165`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`takes[i]` が `finalized[i]` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### INV-REC-202

**要件原文（意図。形式意味との一致は人間が確認する）**

> テイクにも孤児にも、必ず対応する確定済みファイルがある

（出典: `specs/design/project-storage.fsl:203`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="property:invariant:OrphanHasFile#state_rule" digest="sha256:40590fa0fbd686f44be05647eab9abdaadd7393f6ba170273534630c70980b2e" -->
#### 状態不変条件: `OrphanHasFile`

- 識別子: `property:invariant:OrphanHasFile#state_rule`
- 出典: `specs/design/project-storage.fsl:204`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`takes[i] + orphans[i]` が `finalized[i]` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-004

**要件原文（意図。形式意味との一致は人間が確認する）**

> DB へ行を足すのは、ファイル確定が済んでからだけ

（出典: `specs/design/project-storage.fsl:80`）

> コミット済みテイクは減らない

（出典: `specs/design/project-storage.fsl:226`）

> ファイル確定は fsync と rename で行い、DB より先に済ませる

（出典: `specs/design/project-storage.fsl:71`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:commit_take#operation" digest="sha256:0138ba4315bdcacbd3a9e319bb3578e811072071f02fab90695c233437600b17" -->
#### 操作: `commit_take`

- 識別子: `action:commit_take#operation`
- 出典: `specs/design/project-storage.fsl:81`
- パラメータ: なし

操作 `commit_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。
2. `wav` が `Finalized` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `wav` を `NoFile` にする。
3. `takes[i]` を `takes[i] + 1` にする。
4. `item[i]` を `Adopted` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:finalize_file#operation" digest="sha256:14f1c113e29b3828cdecb87f31f9acb9f4b8b88ca856654d5c7a4f858325f0c3" -->
#### 操作: `finalize_file`

- 識別子: `action:finalize_file#operation`
- 出典: `specs/design/project-storage.fsl:72`
- パラメータ: なし

操作 `finalize_file` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。
2. `wav` が `Writing` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `wav` を `Finalized` にする。
2. `finalized[i]` を `finalized[i] + 1` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:TakesAreNeverLost#transition_rule" digest="sha256:b9280030376110b5cf10c5849721af8fe2d2fd835471b309612b3fd856cfd72f" -->
#### 遷移条件: `TakesAreNeverLost`

- 識別子: `property:trans:TakesAreNeverLost#transition_rule`
- 出典: `specs/design/project-storage.fsl:227`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

すべての `i: ListItem` について、`takes[i]` が 遷移前の `takes[i]` 以上である。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-006

**要件原文（意図。形式意味との一致は人間が確認する）**

> コミット前に落ちた確定済み WAV は、孤児として残る

（出典: `specs/design/project-storage.fsl:130`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:crash_leaving_orphan#operation" digest="sha256:06a1d7286fea1fd763139b40d4279279e3d7589f692533987d6829fe2cbf9469" -->
#### 操作: `crash_leaving_orphan`

- 識別子: `action:crash_leaving_orphan#operation`
- 出典: `specs/design/project-storage.fsl:131`
- パラメータ: なし

操作 `crash_leaving_orphan` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。
2. `wav` が `Finalized` である。
3. `crashes` が `MAX_CRASHES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `orphans[i]` を `orphans[i] + 1` にする。
2. `recording` を `none` にする。
3. `wav` を `NoFile` にする。
4. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

### REQ-REC-007

**要件原文（意図。形式意味との一致は人間が確認する）**

> 孤児は、本人が採るか捨てるまで消えない

（出典: `specs/design/project-storage.fsl:241`）

> 本人が要らないと判断したときだけ、孤児を捨てる

（出典: `specs/design/project-storage.fsl:154`）

> 確定済みファイルが減るのは、本人が捨てたときだけ

（出典: `specs/design/project-storage.fsl:233`）

> 起動時の整合性検証が孤児を提示し、本人が復旧として採る

（出典: `specs/design/project-storage.fsl:142`）

**形式化された意味（FSLから決定論的に生成）**

<!-- fsl:claim begin id="action:adopt_orphan#operation" digest="sha256:389b43f50fddebf9ff26bee202d51ddafa80a0f1a12df14c2bf32e716d6b51d0" -->
#### 操作: `adopt_orphan`

- 識別子: `action:adopt_orphan#operation`
- 出典: `specs/design/project-storage.fsl:143`
- パラメータ: `i: ListItem`

操作 `adopt_orphan` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `orphans[i]` が `0` より大きい。
3. `takes[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `orphans[i]` を `orphans[i] - 1` にする。
2. `takes[i]` を `takes[i] + 1` にする。
3. `item[i]` を `Adopted` にする。
4. `ever_recovered` を `true` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:dismiss_orphan#operation" digest="sha256:d8d14407ae2301742568503051d1cc03a0c4a0bb5e4b1bbff90d51b9847d4b27" -->
#### 操作: `dismiss_orphan`

- 識別子: `action:dismiss_orphan#operation`
- 出典: `specs/design/project-storage.fsl:155`
- パラメータ: `i: ListItem`

操作 `dismiss_orphan` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `orphans[i]` が `0` より大きい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `orphans[i]` を `orphans[i] - 1` にする。
2. `finalized[i]` を `finalized[i] - 1` にする。
3. `dismissed[i]` を `dismissed[i] + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:FinalizedFilesOnlyLeaveByChoice#transition_rule" digest="sha256:60839dac5a9669b1507abe5d00d4be9503e8af37db6a6e811a037d588230ed90" -->
#### 遷移条件: `FinalizedFilesOnlyLeaveByChoice`

- 識別子: `property:trans:FinalizedFilesOnlyLeaveByChoice#transition_rule`
- 出典: `specs/design/project-storage.fsl:234`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

すべての `i: ListItem` について、`finalized[i]` が 遷移前の `finalized[i]` 以上である、または、`dismissed[i]` が 遷移前の `dismissed[i]` より大きい。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:OrphanNeverVanishesOnItsOwn#transition_rule" digest="sha256:74a779f0796dca87fe1f553e58a18113247f5c77d00a4bf6f4513c3945b778f3" -->
#### 遷移条件: `OrphanNeverVanishesOnItsOwn`

- 識別子: `property:trans:OrphanNeverVanishesOnItsOwn#transition_rule`
- 出典: `specs/design/project-storage.fsl:242`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の少なくとも一つが成立する。

1. `orphans[i]` が 遷移前の `orphans[i]` 以上である。

2. `takes[i]` が 遷移前の `takes[i]` より大きい。

3. `dismissed[i]` が 遷移前の `dismissed[i]` より大きい。

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

**保証クラス**

- 形式検証: `not_run` — 対応するエビデンスは供給されていない。
- 実装適合: `not_run` — 対応するエビデンスは供給されていない。
- 統計的裏付け: `not_run` — 対応するエビデンスは供給されていない。

## 要件 ID に紐づかない形式要素

次の形式要素は要件 ID に紐づけられていないが、本仕様の一部として検査される。

<!-- fsl:claim begin id="action:crash_losing_part#operation" digest="sha256:5e253f77d50d4f7d4e6fe58daca49f8e7b3a5c19c1b49e6aca5137f598a391d9" -->
#### 操作: `crash_losing_part`

- 識別子: `action:crash_losing_part#operation`
- 出典: `specs/design/project-storage.fsl:120`
- パラメータ: なし

操作 `crash_losing_part` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しくない。
2. `wav` が `Finalized` でない。
3. `crashes` が `MAX_CRASHES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `wav` を `NoFile` にする。
3. `crashes` を `crashes + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:discard_invalid_take#operation" digest="sha256:e80272ac53de51454c685d2ef8ff49f49edc868d1d6edf32cccb94b0b03fd59a" -->
#### 操作: `discard_invalid_take`

- 識別子: `action:discard_invalid_take#operation`
- 出典: `specs/design/project-storage.fsl:90`
- パラメータ: なし

操作 `discard_invalid_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `some` である（その値を `i` と呼ぶ）。
2. `wav` が `Writing` である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `none` にする。
2. `wav` を `NoFile` にする。
3. `invalid[i]` を `invalid[i] + 1` にする。
4. `item[i]` を `if takes[i] > 0 then Adopted else AllInvalid` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:export_zip#operation" digest="sha256:c19e26d937b2594fda68f4742da55a822206a16c421c3b9e5ad385bc65e0a804" -->
#### 操作: `export_zip`

- 識別子: `action:export_zip#operation`
- 出典: `specs/design/project-storage.fsl:105`
- パラメータ: なし

操作 `export_zip` を実行できるのは、次の条件をすべて満たす場合に限る。

1. すべての `i: ListItem` について、`item[i]` が `Adopted` である。
2. `recording` が `none` に等しい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `handoff` を `Exported` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:readopt_earlier_take#operation" digest="sha256:a600c994724b1daab5d669a54a3cc9643af2555e018bb36a544db3f5edf58ede" -->
#### 操作: `readopt_earlier_take`

- 識別子: `action:readopt_earlier_take#operation`
- 出典: `specs/design/project-storage.fsl:99`
- パラメータ: `i: ListItem`

操作 `readopt_earlier_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `takes[i]` が `2` 以上である。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `item[i]` を `Adopted` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:record_more_after_complete#operation" digest="sha256:7a6f06bebd423db91838d0d7eb0ff4f842a8e612b089465e487c3b468248756d" -->
#### 操作: `record_more_after_complete`

- 識別子: `action:record_more_after_complete#operation`
- 出典: `specs/design/project-storage.fsl:111`
- パラメータ: `i: ListItem`

操作 `record_more_after_complete` を実行できるのは、次の条件をすべて満たす場合に限る。

1. すべての `i: ListItem` について、`item[i]` が `Adopted` である。
2. `recording` が `none` に等しい。
3. `invalid[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `invalid[i]` を `invalid[i] + 1` にする。

この操作に公平性の仮定はない。実行可能（enabled）であっても、実行されることは保証されない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="action:start_take#operation" digest="sha256:148a5dbfb07b5bf508a4582a8d6a58967a306a582e872e41fc6f31653d842cad" -->
#### 操作: `start_take`

- 識別子: `action:start_take#operation`
- 出典: `specs/design/project-storage.fsl:61`
- パラメータ: `i: ListItem`

操作 `start_take` を実行できるのは、次の条件をすべて満たす場合に限る。

1. `recording` が `none` に等しい。
2. `wav` が `NoFile` である。
3. `takes[i]` が `MAX_TAKES` より小さい。
4. `invalid[i]` が `MAX_TAKES` より小さい。

操作が成功した場合、次の更新を同一ステップで同時に適用する。更新の右辺は遷移前の状態を読む。

1. `recording` を `some(i)` にする。
2. `wav` を `Writing` にする。

この操作には弱い公平性（weak fairness）を仮定する。これはスケジューリング上の仮定であり、この操作が実行可能（enabled）であり続けるならば、いつかは実行される、という意味である。直ちに実行されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:AdoptedImpliesTake#state_rule" digest="sha256:451eb960b0496b036b103b3c54b401bccc78fd4cf7396fe07788620d50d40668" -->
#### 状態不変条件: `AdoptedImpliesTake`

- 識別子: `property:invariant:AdoptedImpliesTake#state_rule`
- 出典: `specs/design/project-storage.fsl:214`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `Adopted` であるならば、`takes[i]` が `0` より大きい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:AllInvalidHasNoTake#state_rule" digest="sha256:4e50f1b7c179438a0321a36533879fbe5cce214ff426fa2676aadb2ae40f5337" -->
#### 状態不変条件: `AllInvalidHasNoTake`

- 識別子: `property:invariant:AllInvalidHasNoTake#state_rule`
- 出典: `specs/design/project-storage.fsl:218`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `AllInvalid` であるならば、`takes[i]` が `0` に等しい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:CrashesBounded#state_rule" digest="sha256:953a0d586f1e3c53fd840fe342157ae332f17b1503a4ea392140ccce1f2a7d33" -->
#### 状態不変条件: `CrashesBounded`

- 識別子: `property:invariant:CrashesBounded#state_rule`
- 出典: `specs/design/project-storage.fsl:184`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`crashes` が `MAX_CRASHES` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:FinalizedAccountedFor#state_rule" digest="sha256:197916e42e1e8ffb082fcee39c46fbc9967ee775cc551cbd0b3c168d090daf7b" -->
#### 状態不変条件: `FinalizedAccountedFor`

- 識別子: `property:invariant:FinalizedAccountedFor#state_rule`
- 出典: `specs/design/project-storage.fsl:188`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
forall i: ListItem { finalized[i] <= takes[i] + orphans[i] + (if wav == Finalized and recording == some(i) then 1 else 0) }
```

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:FinalizedAheadOfCommitted#state_rule" digest="sha256:c9295f62136d3e6ce371623e28bcad7a4904d2ad1fbfc9fadd71a18998461572" -->
#### 状態不変条件: `FinalizedAheadOfCommitted`

- 識別子: `property:invariant:FinalizedAheadOfCommitted#state_rule`
- 出典: `specs/design/project-storage.fsl:170`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、（`wav` が `Finalized` である、かつ、`recording` が `some(i)` に等しい）ならば、`takes[i] + orphans[i]` が `finalized[i]` より小さい。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:OrphansBounded#state_rule" digest="sha256:f860a0cbd5a323ec77ad39e412222f6071a782b0e69bd8cba87ec6b7ef5ba794" -->
#### 状態不変条件: `OrphansBounded`

- 識別子: `property:invariant:OrphansBounded#state_rule`
- 出典: `specs/design/project-storage.fsl:197`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

各 `i: ListItem` にわたる `orphans[i] + dismissed[i]` の合計 が `crashes` 以下である。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:RecordingImpliesRoom#state_rule" digest="sha256:1a81bad944b1e6d58ab7d159976c1e36f585f4cf6c3510f593ca3a63aeca5a12" -->
#### 状態不変条件: `RecordingImpliesRoom`

- 識別子: `property:invariant:RecordingImpliesRoom#state_rule`
- 出典: `specs/design/project-storage.fsl:208`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`recording` が `some(i)` に等しいならば、（`takes[i]` が `MAX_TAKES` より小さい、かつ、`invalid[i]` が `MAX_TAKES` より小さい）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:UnrecordedHasNothing#state_rule" digest="sha256:7444d9ac012da58071f01ec9cd4385f405c1ce2540d088b00024a5a3236487ef" -->
#### 状態不変条件: `UnrecordedHasNothing`

- 識別子: `property:invariant:UnrecordedHasNothing#state_rule`
- 出典: `specs/design/project-storage.fsl:222`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

すべての `i: ListItem` について、`item[i]` が `Unrecorded` であるならば、（`takes[i]` が `0` に等しい、かつ、`invalid[i]` が `0` に等しい）。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:invariant:WavOnlyWhileRecording#state_rule" digest="sha256:b8738d83f5cae14cb3c757732a08da231db5a8f3e02cb9b9fb0cefb3be1aee05" -->
#### 状態不変条件: `WavOnlyWhileRecording`

- 識別子: `property:invariant:WavOnlyWhileRecording#state_rule`
- 出典: `specs/design/project-storage.fsl:177`

初期化後、および成功した各操作のコミット後に、次の条件が成立しなければならない。

`wav` が `NoFile` でないならば、`recording` が `none` に等しくない。

この条件を満たさない候補遷移はコミットされない。条件が自動的に修復・回復されることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:CompleteWhileUnpublished#reachability_goal" digest="sha256:09f96aebfb9d208b44baa83a0b0f8bc312b811264e31b4ed41c2f6a54bf78de1" -->
#### 到達目標: `CompleteWhileUnpublished`

- 識別子: `property:reachable:CompleteWhileUnpublished#reachability_goal`
- 出典: `specs/design/project-storage.fsl:267`

次の状態に到達する実行例が存在しなければならない（到達目標）。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
(forall i: ListItem { item[i] == Adopted }) and handoff == NotExported
```

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:reachable:OrphanRecoveredByUser#reachability_goal" digest="sha256:e723a696a83c9931f6114dc518f557514742995029ffbab19bd1cdcd799b763c" -->
#### 到達目標: `OrphanRecoveredByUser`

- 識別子: `property:reachable:OrphanRecoveredByUser#reachability_goal`
- 出典: `specs/design/project-storage.fsl:263`

次の状態に到達する実行例が存在しなければならない（到達目標）。

`ever_recovered` が `true` である。

これは「少なくとも一つの実行が存在する」ことを求める到達目標であり、すべての状態での成立を求める不変条件ではない。

- 検証状態: この規範文自体は検証結果を含まない。検証エビデンスが供給されている場合はこの要件の「保証クラス」欄に別掲され、供給されていない場合は `not_run` と明示される。到達が確認済みであることを意味しない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:CrashKeepsCommittedTakes#transition_rule" digest="sha256:cd494b3ea72bb765da65d79c021df4d7ee6111495506f7b306dbf45b2c2e7dd6" -->
#### 遷移条件: `CrashKeepsCommittedTakes`

- 識別子: `property:trans:CrashKeepsCommittedTakes#transition_rule`
- 出典: `specs/design/project-storage.fsl:258`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
crashes != old(crashes) => (forall i: ListItem { takes[i] == old(takes[i]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ExportDoesNotChangeItems#transition_rule" digest="sha256:c709849e1bcec2f08ea8856d5b352bf6befb2806830ad1a5bbeecebcdf9413c0" -->
#### 遷移条件: `ExportDoesNotChangeItems`

- 識別子: `property:trans:ExportDoesNotChangeItems#transition_rule`
- 出典: `specs/design/project-storage.fsl:254`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
handoff != old(handoff) => (forall i: ListItem { item[i] == old(item[i]) })
```

この関係を満たさない候補遷移はコミットされない。
<!-- fsl:claim end -->

<!-- fsl:claim begin id="property:trans:ExportOnlyWhenComplete#transition_rule" digest="sha256:dd5c218181995fb17c0bef981aaf863e422e35625bfa8f9d2f63d7d5b88df756" -->
#### 遷移条件: `ExportOnlyWhenComplete`

- 識別子: `property:trans:ExportOnlyWhenComplete#transition_rule`
- 出典: `specs/design/project-storage.fsl:250`

成功する各遷移について、遷移前の状態と遷移後の状態は次の関係を満たさなければならない。以下で「遷移前の `x`」は遷移前の値を指し、それ以外の読み取りは遷移後の値を指す。

次の条件が成立する（FSL canonical 形式で示す）。

```fsl
handoff != old(handoff) => (forall i: ListItem { item[i] == Adopted })
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

本仕様に解析スコープの宣言（instances / values）はない。

## 生成情報

- 生成元仕様: `specs/design/project-storage.fsl`（`KoeruProjectStorage`、dialect: `spec`）
- spec digest: `sha256:1606c38c2884202c3531da7dc6db999dfea16f6ad09b7746708f66dbc170c072`
- claim set digest: `sha256:9cf110ac21bb11e246a9cbef6236b77c0758f6a203d6c7084e9ae782bb658b84`
- 形式要素の分類: rendered 10 件 / unattributed 21 件 / unsupported 1 件
- 自然言語への言い換えを行わなかった式: 6 箇所
- 由来情報は不完全である（completeness: `Partial`）。一部の要素について、FSL ソース上の出所を特定できていない。
- 上記 6 箇所では、誤解を招く言い換えを避けるため、自然言語文の代わりに FSL の canonical 形式をそのまま示した。これは本レンダラーの仕様どおりの動作であり、情報の欠落や生成の失敗ではない。
- 次の形式要素は RCIR v1 が対応していないため、本書には規範文として現れない。省略は明示され、黙って落とされることはない。
- `init`: no v1 claim kind projects initial-state definitions
