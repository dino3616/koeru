# KOERU Design System / xtask — Dependency-aware Parallel Agent Work Plan

> [!NOTE]
> この文書は、現在の KOERU repository に対して **xtask を再設計し、その基盤上へ KOERU Design System を複数 Agent で並列実装するための delivery plan** である。
> Product rollout の段階導入計画ではない。
>
> Design System の意味論は [koeru_design_system_proposal_full.md](./koeru_design_system_proposal_full.md)、
> KOERU 本体の architecture refactor は [../architecture/03-task-dag.md](../architecture/03-task-dag.md) を正本候補として参照する。
> この計画は、それらを実装へ接続する時点依存の作業分解である。

# 0. Goal

現在の KOERU には二つの大きな変更が同時にある。

1. Product runtime / consumer contract の architecture refactor。
2. Repository semantics / Design System / verification workflow の再設計。

この二つを別々の tooling で実装しない。

現在の `xtask/src/main.rs` は meta schema、reference、Git diff、ID allocation、budget/coverage/profile check、test portfolio、CLI dispatch を一箇所に持つ。
Design System の Context / semantic graph / Probe / migration をそのまま追加すると、複数 Agent が同じ file と同じ raw TOML helper を変更する。

したがって先に `xtask` を **Repository Knowledge / Verification substrate** として rearchitecture し、
既存 architecture plan の T15 と Design System の両方をその consumer にする。

成功状態は次である。

```text
meta / specs / source / dependency manifests / git revisions
                      |
                      v
              Repository View
                      |
                      v
              Knowledge Snapshot
                      |
             +--------+--------+
             |                 |
             v                 v
       Semantic Graph      Probe Registry
             |                 |
      +------+------+          v
      |      |      |       Runner
      v      v      v          |
   Context Pattern Views     Receipt
             |                 |
             +--------+--------+
                      |
                      v
          deterministic checks / CI
                      |
                      v
                 Agent Skills
```

`xtask` は semantics の正本ではない。
正本は meta / FSL / code / manifest / explicit operational definitions にあり、
`xtask` はそれらを deterministic に読み、検査し、projection を作る。

# 1. Non-goals

この実装計画では次をしない。

- Design System 専用の第二 `xtask` / validator framework を作らない。
- `Q/HYP/EVID/DEC/REQ` と `PROBE/WORKLOAD/Receipt` を同じ種類の object として扱わない。
- Pattern / Budget / Component Inventory / Verification Portfolio を手書きの第二正本にしない。
- `BUDGET/TGT/SCALE/CMP/SUITE` を既存だからという理由だけで温存しない。
- architecture refactor T02–T14 を Design System 完成待ちにしない。
- T17 の contributor Agent Skills と Design System C9 の Agent capability を同一 task にしない。
- bulk rename / migration を graph / Context / Probe substrate より先に行わない。
- Agent ごとに `xtask/src/main.rs`、schema vocabulary、root config を独自変更させない。

# 2. Existing KOERU architecture plan との接続

現在の architecture refactor では T15 が CI / xtask / test portfolio / schema gate の owner である。
T17 は Agent Skills を workflow/control plane とし、canonical semantics を Skill 内へコピーしない。

この計画では、T15 が巨大な現行 `xtask` に gate を積み増すのではなく、
X-track が作る Repository Knowledge / Probe-Receipt substrate を使う。

```text
Product architecture:
T04 .. T14 -------------------------------+
                                           |
xtask rearchitecture:                      |
X00 -> X01 -> X02/X03/X04 -> X05 -> X07 -+--> T15 --> T16 --> T17
                       \        \
                        \        +-> X06 migration engine
                         \
Design System:            +-> D00 -> D01/D02/D03/D04
                                  |      |
                                  |      +-> D07c verification views
                                  +-> D05/D06/D07a/D07b
                                           |
                                           v
                                          D08 semantic cutover
                                           |
                                           v
                                          D09 C9 skills

All required tracks -----------------------------------------> D10 integration
```

T15 は Design System の Pattern View や C9 Agent を待たない。
T15 が必要とするのは shared xtask substrate、Probe / Receipt、既存 architecture gate の deterministic execution である。

# 3. Delivery protocol for parallel Agents

## 3.1 Branch / worktree

各 work package は別 worktree と別 branch で行う。
同じ working tree を複数 Agent が共有しない。

推奨 branch:

```text
integration/design-system-xtask

agent/x01-kernel
agent/x02-repo-view
agent/x03-knowledge
agent/x04-probe-receipt
agent/x05-graph
agent/x06-migration
agent/x07-existing-checks

agent/d00-ontology
agent/d01-context
agent/d02-verification
agent/d03-github
agent/d04-ui-boundary
agent/d05-reentry
agent/d06-pattern-view
agent/d07-derived-views
agent/d08-cutover
agent/d09-design-agents
agent/d10-integration
```

## 3.2 Shared contract owner

次の public contract は single owner で変更する。

| Contract | Owner |
|---|---|
| `RepoView`, `RevisionRef`, process adapter boundary | X01 / X02 |
| `KnowledgeSnapshot`, raw document / typed object boundary | X03 |
| `Diagnostic`, machine-readable result envelope | X01 |
| `ProbeDefinition`, `ExecutionContext`, `Receipt` | X04 |
| `GraphNode`, `Relation`, `SemanticGraph`, graph diff | X05 |
| migration plan / rewrite transaction | X06 |
| Design System object kinds / canonical relation registration | D00 |
| Context JSON / Markdown envelope | D01 |
| Evidence capability vocabulary | D02 |
| checkpoint shape | D05 |
| Pattern candidate shape | D06 |
| Budget / Component / Verification view output | D07 |
| old→new ID mapping during cutover | D08 |

interface 変更が必要な下流 Agent は shared file を直接変更しない。
次を handoff する。

```text
requested interface change
why current contract is insufficient
producer / consumer affected
negative fixture
migration impact
```

## 3.3 Required Agent handoff

各 Agent は最低限次を返す。

```text
base SHA
head SHA
owned files
public interfaces changed
commands/tests actually executed
commands/tests not executed and why
acceptance fixtures added
open assumptions / blocked dependencies
human/native/perceptual work still required
interface change requests
```

未実行を pass と書かない。
test binary の存在、mock、skipped test、CI job の起動だけを execution evidence にしない。

# 4. Target xtask structure

最初から crate を量産しない。
まず `xtask` package 内で library boundary を作り、必要性が実測された場合だけ後で crate 化する。

```text
xtask/src/
  main.rs                 # thin binary only
  lib.rs
  cli.rs

  repo/
    mod.rs
    view.rs               # working tree / git revision
    git.rs
    scan.rs
    process.rs            # external process boundary

  knowledge/
    mod.rs
    model.rs              # generic typed records
    load.rs
    ids.rs
    refs.rs
    fsl.rs
    design.rs             # D00: Q/HYP/EVID/DEC/REQ + legacy adapters

  graph/
    mod.rs
    build.rs
    diff.rs

  probe/
    mod.rs
    model.rs
    registry.rs
    runner.rs
    receipt.rs

  checks/
    mod.rs
    meta.rs
    references.rs
    architecture.rs
    schema.rs

  context/
    mod.rs
    compile.rs
    render.rs

  verification/
    mod.rs
    capability.rs
    debt.rs

  views/
    pattern.rs
    budget.rs
    components.rs
    portfolio.rs
    coverage.rs

  migration/
    mod.rs
    plan.rs
    rewrite.rs
    verify.rs
```

この path は implementation ownership の target であり、file 数そのものを architecture goal にしない。

# 5. X-track — xtask rearchitecture

## X00 — Current xtask characterization

- **Objective:** 現在の command / output / failure semantics を固定し、refactor 中の accidental behavior change を検出する。
- **Prerequisites:** なし。
- **Owned / affected:** fixture と baseline receipt のみ。
- **Must not change:** production/meta/spec data、command semantics。
- **Work:** `check-meta`, `check-budgets`, `check-coverage`, `check-references`, `check-profile`, `index-decisions`, `check-portfolio`, `test-receipt`, `touched`, `next-id`, `dump-requirements`, `check-schema` の representative behavior を capture する。
- **Acceptance:** success / failure / not-run を区別した baseline があり、current main SHA が記録される。
- **Handoff:** X01。architecture T00 の baseline が利用できる場合は重複計測せず参照する。

## X01 — Library kernel / CLI seam

- **Objective:** `main.rs` を command orchestration だけにし、並列 Agent が同じ root file を触らなくて済む seam を作る。
- **Prerequisites:** X00。
- **Owned:** `xtask/src/main.rs`, `lib.rs`, `cli.rs`, root module declarations、`Diagnostic`。
- **Must not change:** existing check meaning、meta schema meaning、runner behavior。
- **Work:**
  - command dispatch と service construction を分離する
  - current `Report` を typed Diagnostic + human renderer に移せる boundary を作る
  - module slots を先に作る
  - downstream module から `main.rs` private helper を import しない
- **Tests:** X00 baseline parity。
- **Acceptance:** X02–X07 が `main.rs` を変更せず追加できる。
- **Human review:** abstraction が current command 数に対して過剰でないこと。

## X02 — Repository View / revision adapters

- **Objective:** working tree と任意 Git revision を同じ read interface から読む。
- **Prerequisites:** X01。
- **Owned:** `repo/**`。
- **Must not change:** semantic schema、Context logic、migration。
- **Work:**
  - `RepoView::working_tree`
  - `RepoView::revision(<sha/ref>)`
  - read / exists / scoped walk / revision identity
  - git diff / source scan
  - Cargo/Bun/Nix 等の external command は process adapter の後ろに置く
- **Tests:** deleted file、renamed file、old revision、dirty working tree、missing ref。
- **Acceptance:** semantic consumer が shelling out to `git show` を直接行わない。
- **Handoff:** X03 / X05 / D01 / D07。

## X03 — Knowledge Snapshot / loaders / indices

- **Objective:** raw TOML / FSL / manifest から consumer-independent な read model を作る。
- **Prerequisites:** X01。X02 と並列実装可能、integration 時に RepoView を使う。
- **Owned:** `knowledge/model.rs`, `load.rs`, `ids.rs`, `refs.rs`, `fsl.rs`。
- **Must not change:** Design System ontology の意味。D00 が registration owner。
- **Work:**
  - current `Shape / Entry / list_of / str_of` の root-private coupling を除く
  - source location / schema / identity / relation source を typed にする
  - legacy meta shapes を lossless に読める
  - upper layer へ `toml::Table` を漏らさない
- **Tests:** unknown schema、duplicate ID、broken file、empty collection、legacy aggregate file。
- **Acceptance:** current checks と D00 が同じ KnowledgeSnapshot を使える。
- **Handoff:** X05 / X06 / X07 / D00。

## X04 — Probe / Runner / Receipt substrate

- **Objective:** current `SUITE` / `receipt.rs` の能力を、test suite 固有ではない reusable Probe execution model へ一般化する。
- **Prerequisites:** X01。X02 と並列可能。
- **Owned:** `probe/**`、runner adapters。
- **Must not change:** human-only verification を automation に変える、missing fixture を pass にする。
- **Work:**
  - `ProbeDefinition`
  - platform / backend / runner / fixture prerequisites
  - discovered / executed / actual-work counters
  - tested SHA / environment / fixture hashes
  - passed / failed / skipped / not-applicable / not-run を区別
  - Cargo/Bun を最初の runner adapter とする
- **Tests:** 0 discovery、early return、missing LFS/model、unsupported backend、mock-only、manual-only。
- **Acceptance:** existing cargo/ui suite behavior を compatibility adapter 経由で表現でき、run result は structured Receipt になる。
- **Handoff:** T15 / D02 / D07。

## X05 — Semantic Graph core

- **Objective:** KnowledgeSnapshot と operational locator から revision-specific typed graph を作り、比較できるようにする。
- **Prerequisites:** X02 + X03。D00 の ontology 実装を待たず fixture node kinds で開発できる。
- **Owned:** `graph/**`。
- **Must not change:** object semantics、Graph から自動的に normative knowledge を生成する。
- **Work:**
  - node / relation / source locator
  - revision-specific graph build
  - graph neighborhood traversal primitives
  - old/new graph union diff
  - added / removed / changed / superseded / unresolved
- **Tests:** cycle、dangling edge、deleted node、replacement chain、same text/different identity、old schema。
- **Acceptance:** graph engine は Q/HYP 等の具体語を hardcode せず registration から扱える。
- **Handoff:** D01 / D05 / D06 / D07 / X06 / X07。

## X06 — Migration planner / rewrite engine

- **Objective:** schema / ID cutover を ad-hoc search-replace ではなく plan → dry-run → rewrite → reload verify で行う。
- **Prerequisites:** X02 + X03 + X05。
- **Owned:** `migration/**`。
- **Must not change:** migration rule 自体。D08 が mapping policy owner。
- **Work:**
  - rewrite plan
  - collision detection
  - source + inbound reference rewrite
  - dry-run
  - before/after graph equivalence hooks
  - partial failure で working tree を中途半端に publish しない
- **Tests:** collision、unknown inbound ref、same file multiple refs、FSL + TOML + source refs、restart after failed dry-run。
- **Acceptance:** D08 が mapping table を供給するだけで migration engine を再実装しない。

## X07 — Existing check migration / T15 substrate

- **Objective:** 現行 `main.rs` の checks を X01–X05 の substrate へ載せ替え、T15 が Design System 専用コードなしで使える状態にする。
- **Prerequisites:** X03 + X04 + X05。X02 integration。
- **Owned:** `checks/**` と既存 command compatibility layer。
- **Must not change:** current product semantics、architecture T15 の hard/advisory 判定を勝手に変更。
- **Work:**
  - check-meta / references / schema
  - architecture import/dependency/SDL checks の integration seam
  - legacy budget/component/suite checks は cutover まで compatibility view から実行
  - machine-readable diagnostics / receipts
- **Tests:** X00 parity + negative canary。
- **Acceptance:** T15 が giant `main.rs` へ新 validator を足さずに gate を追加できる。
- **Handoff:** T15 / D10。

# 6. D-track — KOERU Design System

## D00 — Design System ontology / legacy graph adapter

- **Objective:** Target semantic object と operational resource を Knowledge/Graph substrate へ登録する。
- **Prerequisites:** X01 contract。implementation integration は X03 + X05。
- **Owned:** `knowledge/design.rs` と design-specific fixtures。
- **Must not change:** generic graph / RepoView interface。
- **Semantic objects:** `Q / HYP / EVID / DEC / REQ`。
- **Operational resources:** `PROBE / WORKLOAD / source / story / dependency / artifact / PR / commit / Receipt locator`。
- **Canonical relations:** `resolved_by`, `relies_on_hypotheses`, `targets`, `derived_from`, `assesses`, `considers`, `establishes/revises/removes`, `constrains`, `formalizes`, `provenance`, `implements`, `context_for`。
- **Legacy read:** TR / FSL REQ / BUDGET / TGT / SCALE / CMP / SUITE を target meaning へ lossless に expose する compatibility adapter。
- **Acceptance:** downstream consumer は legacy prefix / raw TOML layout を知らない。

## D01 — Current / Comparative Context

- **Objective:** 一つの `context` command で current semantic state と baseline からの semantic changes を返す。
- **Prerequisites:** X02 + X05 + D00。
- **Owned:** `context/**`。
- **Must not change:** graph meaning / migration / GitHub history。
- **Work:**
  - `context --root <root> --at <revision>`
  - optional `--from <revision>`
  - current + comparative output
  - normative ancestors / empirical dependencies / contradiction / open Q / provenance locator
  - removed relation を old/new neighborhood union から保持
  - Markdown / JSON projection
- **Tests:** UNMAPPED、deleted edge、superseded DEC、contradicting EVID、legacy+target mixed revision、token budget。
- **Acceptance:** `--from` があっても current state を省略しない。

## D02 — Verification semantics / Evidence capability

- **Objective:** reusable Probe が何を証言できるかを HYP / REQ と接続し、false evidence を防ぐ。
- **Prerequisites:** X04 + D00。graph integration は X05。
- **Owned:** `verification/**`, `verification/probes/**` schema。
- **Must not change:** HYP truth、REQ acceptance、human judgement。
- **Work:**
  - Probe target / capability
  - HYP kind × method capability registry
  - human-required / native-required
  - Receipt から durable EVID への canonization boundary
  - Human Verification Debt
- **Tests:** skipped native、synthetic novice、browser mock、actual-work 0、same-origin evidence、human-only HYP。
- **Acceptance:** Probe definition と Receipt の存在だけで Evidence supported にできない。

## D03 — GitHub / contributor process surface

- **Objective:** process/history を GitHub に置き、semantic graph へ必要な結果だけ canonize できる入口を作る。
- **Prerequisites:** D00 vocabulary。
- **Owned:** `.github/ISSUE_TEMPLATE/design-signal.yml`, PR template の Design System 欄、`design/policy.toml`。
- **Must not change:** Issue text だけで Agent authority を増やす、process transcript を meta に複製する。
- **Acceptance:** Q/HYP/DEC/REQ/provenance root を記載でき、exploration / critique / rejected alternatives は Issue/PR に残せる。

## D04 — UI / Storybook boundary and design-links

- **Objective:** stable artifact と branch-local exploration を分離し、UI source/story から semantic root を解決する。
- **Prerequisites:** D00 locator contract。
- **Owned:** `crates/koeru-app/ui/design-links.toml`, Storybook stable/workbench boundary、experiment import check。
- **Must not change:** production UX を plan 実装のついでに redesign しない。
- **Acceptance:** production から experiment import は fail。stable story だけ durable mapping に載る。

## D05 — Re-entry / checkpoint

- **Objective:** saved baseline と current Context から復帰できる。
- **Prerequisites:** D01。
- **Owned:** re-entry command/module、local checkpoint schema。
- **Must not change:** contributor profile DB を作る、本人の過去理解を推測する。
- **Acceptance:** missing baseline / deleted root / known successor / unknown successor を区別する。

## D06 — Derived Pattern View

- **Objective:** PAT object を作らず graph の反復構造を candidate として出す。
- **Prerequisites:** X05 + D00。Context cross-check は D01。
- **Owned:** `views/pattern.rs`。
- **Must not change:** source DEC/REQ、Pattern candidate に authority を与える。
- **Acceptance:** candidate は source object IDs を必ず持ち、source が superseded されたら view も更新される。

## D07 — Derived operational views

D07 は同じ shared graph を読むが、file ownership を分けて並列化できる。

### D07a — Budget View

- **Prerequisites:** D00 + X05。
- **Owned:** `views/budget.rs`。
- **Migration source:** BUDGET / quantitative TGT / SCALE。
- **Acceptance:** quantitative REQ + workload + allocation relation から limit / subtotal / mode peak / margin を再計算できる。

### D07b — Component Inventory / Requirement Coverage

- **Prerequisites:** X02 + D00 + X05。
- **Owned:** `views/components.rs`, `views/coverage.rs`。
- **Migration source:** CMP ledger / dependency manifests / lock files。
- **Acceptance:**
  - package の存在/version は repository から得る
  - 採否理由は DEC に戻る
  - license/measurement は EVID へ戻る
  - Requirement coverage を `needs_component` boolean で表さない
  - `formalizes / implements / Probe targets` の coverage を別々に出す

### D07c — Verification Portfolio / Debt

- **Prerequisites:** X04 + D02。
- **Owned:** `views/portfolio.rs`, verification debt projection。
- **Migration source:** SUITE portfolio。
- **Acceptance:** required Probe と executed Receipt を突き合わせ、platform/backend/fixture/actual-work の不足を false green にしない。

## D08 — Semantic cutover / legacy object decomposition

- **Objective:** read path と derived views が成立した後、一回の controlled migration で repository 正本を target model へ切り替える。
- **Prerequisites:** X06 + D01 + D02 + D07a/b/c。D06 は blocking ではない。
- **Owned:** actual `meta/**`, `specs/**`, `verification/**`, reference rewrite、meta/spec README。
- **Must not change:** semantic statement と ID rename を一つの review 不能な差分へ混ぜる。
- **Migration:**
  - `TR-* → REQ-<opaque>`
  - FSL `REQ-* → OBL-<opaque>`
  - FSL `INV/FB` domain-free opaque identity
  - DEC/Q/EVID → domain-free opaque identity
  - HYP target files
  - BUDGET → quantitative REQ + allocation data/view
  - TGT → REQ/HYP/EVID/DEC
  - SCALE → workload + DEC/EVID + calculation
  - CMP → repository inventory + DEC/EVID; ledger removal
  - SUITE → reusable PROBE definitions; receipts remain generated
- **Verification:** before/after Context, current check behavior, budget totals, required test/probe coverage、all inbound refs。
- **Acceptance:** legacy adapter を無効にした target fixture でも全 deterministic checks が成立する。
- **Human review:** ambiguous TGT/CMP item の type assignment。意味を機械的に推測しない。

## D09 — Design System C9 Agent capabilities

- **Objective:** deterministic substrate の上に bounded Agent modes を載せる。
- **Prerequisites:** D01 + D02 + D03 + D05。Pattern explanation は D06、portfolio explanation は D07c。
- **Owned:** `.agents/skills/design-frame/**`, `design-explore/**`, `design-review/**`, `design-context-audit/**`, `design-verify/**`, `design-reentry/**`。
- **Must not change:** accepted DEC/REQ、Evidence truth、merge/release authority。T17 の contributor architecture Skills をコピーしない。
- **Acceptance:** Agent を削除しても CLI / GitHub / Storybook / deterministic checks で Design System が成立する。

## D10 — Integration / acceptance

- **Objective:** X-track / D-track / architecture T15 の組み合わせが一つの repository model として働くことを検証する。
- **Prerequisites:** X01–X07、D00–D08。D09 は core acceptance 後に同じ matrix で追加確認してよい。
- **Owned:** integration fixes、CI registration、acceptance fixture index。
- **Must not change:** failing fixture を削除して green 化、shared contract を integration owner が無断再定義。
- **Acceptance:** Section 8 の matrix を満たす。
- **Handoff:** T15 hard/advisory gate 判定、T17 contributor Skills、Design System C10 health monitoring。

# 7. Integration barriers

これは product rollout の stage ではなく、parallel branch を安全に merge する同期点である。

## I0 — Fan-out

X00 / X01 を merge し shared interface SHA を固定する。
X02 / X03 / X04 / X05-fixture / D00-fixture / D03 / D04 が fan-out できる。

## I1 — Knowledge substrate

X02 + X03 + X04 + X05 + D00 を integration branch へ集約する。
ここで raw TOML helper が upper module に漏れていないこと、Probe/Receipt と SemanticGraph が別 layer であることを確認する。

## I2 — Read-only Design System

D01 / D02 / D05 / D06 / D07a/b/c を統合する。
この時点では legacy meta を read-only compatibility で読み、bulk migration はまだ行わない。

## I3 — Controlled cutover

X06 dry-run と D08 mapping を review し、単一 owner が repository-wide rewrite を実施する。
複数 Agent が同時に canonical IDs を rename しない。

## I4 — Governance / Agents

D03/D04 の surface と D09 Agent capability を統合する。
Agent は deterministic source の consumer としてのみ追加する。

# 8. Acceptance matrix

| Fixture / adversarial case | Primary owner | Required behavior |
|---|---|---|
| old command behavior changes during xtask split | X01/X07 | X00 baseline と意図しない差があれば fail |
| old revision にしか node がない | X02/X05 | comparative graph で removed として保持 |
| source に ID 引用がない | D01 | impact なしではなく `UNMAPPED` |
| accepted DEC の selected を意味変更で上書き | D00/X07 | new DEC + supersedes を要求 |
| `formalizes` を完全同値として扱う | D00/D01 | partial coverage として表示 |
| HYP に contradicting EVID がある | D01/D02 | supporting と同じ Context 面に表示 |
| native Probe が fixture 無しで return | X04/D07c | passed ではなく not-run / failed prerequisite |
| browser mock だけで native REQ を verified | D02 | capability mismatch |
| human-only HYP に synthetic novice | D02 | support 不可 |
| Probe registry はあるが Receipt がない | D07c | planned / not-executed |
| BUDGET memory peak | D07a | current legacy calculation と target View が同じ peak を出す |
| TGT item の意味が ambiguous | D08 | auto migration せず blocked |
| SCALE derived formula | D07a/D08 | workload source と derived value を分離し再計算可能 |
| CMP adopted dependency absent from manifest | D07b | inventory inconsistency |
| CMP rejected candidate | D08 | persistent component node を要求せず provenance/DEC へ戻す |
| Requirement without external component | D07b | `needs_component=false` を要求しない |
| filename stem != semantic ID | D00/D08 | fail |
| content edit changes opaque ID | D00 | fail |
| Pattern candidate loses source | D06 | fail |
| `context --from` returns changes-only | D01 | fail |
| production imports experiment | D04 | fail |
| Issue text requests extra Agent permission | D03/D09 | ignore / reject |
| Agent states human verification was performed | D09 | source Receipt/EVID がなければ禁止 |

# 9. Migration rules for current meta objects

## 9.1 BUDGET

Budget limit は quantitative REQ へ移す。
allocation のうち future work を拘束する値だけ REQ / relation にし、subtotal / peak / margin は derived View にする。
`measured=false` の allocation を Evidence にしない。

## 9.2 TGT

一括 rename しない。
各 target を意味で分類する。

```text
normative threshold -> REQ
prediction          -> HYP
observation         -> EVID
choice of target    -> DEC
```

分類不能なら Q / migration blocker とする。

## 9.3 SCALE

representative workload の選択根拠は DEC。
測定済み basis は EVID。
再利用する workload definition は `verification/workloads/`。
式で出る値は generated calculation。

## 9.4 CMP

package/version は Cargo/Bun/Nix/submodule/asset manifest から inventory。
採用 / 不採用は DEC。
license / benchmark / upstream fact は EVID。
探索途中の候補は Issue/PR。
`CMP-*` を dependency identity として維持しない。

## 9.5 SUITE

既存 test target 登録は reusable PROBE へ変換する。
`SUITE` という語は test implementation の grouping を指す場合にだけ使い、repository knowledge ID としては廃止する。

```text
PROBE = reusable verification method
Receipt = one execution
EVID = durable reusable observation
```

# 10. T15 / T17 boundary

## T15

T15 は X04 Probe/Receipt と X07 checks を利用する。
「required suite が登録されているか」ではなく、
**required Probe が存在し、対象 environment で必要な Receipt / actual work が得られたか**を gate にする。

Budget / component / portfolio の view は Design System 由来でもよいが、T15 hard gate にする値は明示 DEC/REQ で採択されている必要がある。

## T17

T17 の `plan-koeru-change` / `verify-koeru` は canonical rules を保持しない。
Context / Probe / Receipt / owner graph の deterministic output を読む。

D09 の Design System Agent は exploration / critique / re-entry を扱う別 capability であり、
T17 の architecture contributor Skills と統合する必要はない。

# 11. Completion criteria

この implementation effort は、次を満たしたら完了とする。

1. `xtask/src/main.rs` が schema / graph / runner business logic の owner ではない。
2. working tree と arbitrary revision が同じ RepoView / Knowledge pipeline で読める。
3. legacy と target object が同じ semantic graph API から読める。
4. Context が current / comparative の一つの command で生成できる。
5. reusable Probe と run Receipt が分離され、false green を機械的に検出できる。
6. durable EVID は Receipt の全履歴ではなく、再利用価値のある観測だけを保持する。
7. BUDGET/TGT/SCALE/CMP/SUITE の意味が target model へ分解され、旧 object type が新規知識の追加先になっていない。
8. Pattern / Budget / Component Inventory / Verification Portfolio は source graph から再生成できる。
9. bulk migration は dry-run / collision / inbound reference / before-after verification を通る。
10. T15 が同じ xtask substrate で architecture fitness gate を実行できる。
11. Agent を無効にしても Design System と architecture gate が成立する。
12. implementation Agent が互いの owned files を通常変更せず並列作業できる。
