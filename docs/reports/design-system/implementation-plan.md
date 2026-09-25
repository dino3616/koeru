# KOERU Design System — Implementation Plan

> [!NOTE]
> この文書は Design System の Reference Architecture を実装へ移すための時点依存の計画である。
> 理想形・研究根拠・Execution Contract の正本候補ではない。順序・対象・完了条件は実装から学んで更新する。
> Architecture は [koeru_design_system_proposal_full.md](./koeru_design_system_proposal_full.md) を参照。

# 17. 明日からの実装順序

最初から全 schema・bot・dashboard を作らない。最初の一件で社会的な運用が成立することを確かめ、その後に繰り返し部分を自動化する。

| 順序 | Repository に入れるもの | 完了条件 |
|---|---|---|
| **PR 1：判断の境界を明確にする** | Vision と `meta/README` の正本関係、実験 workbench を許す限定的な Decision、Signal Form、短い PR 欄 | 新 Contributor が、方針を守る変更と方針を疑う提案の両方を出せる |
| **PR 2：最小 Context compiler** | 既存 ID と `touched` を使う `context`、不足表示、基本 graph fixture、`--from` による comparative view | 参照のない UI 変更を「影響なし」と表示せず、current state と semantic changes を同じ計算モデルで出す |
| **PR 3：一つの実際の探索を通す** | Voice 画面の Question、Issue / PR 上の比較・critique、branch-local story、最小の Hypothesis／Evidence／Decision。必要な場合だけ current obligation を Requirement として分離 | 過程を別 Markdown に複製せず、一件の変更が問いから実装まで通り、DEC と REQ に同じ規範文を重複させない |
| **PR 4：Hypothesis と検証範囲を型にする** | `HYP` schema、Evidence の method／実行状態、`check-design` | synthetic user と skipped test を、利用者観察・実音声確認へ昇格できない |
| **PR 5：Re-entry と handoff** | checkpoint、`context --from <baseline> --at HEAD`、旧・新 schema fixture | current Context と、削除された関係・後継判断を含む semantic changes が同じ出力に出る |
| **PR 6：Object identity と requirement layer を移行** | collision-checked opaque ID generator、`filename stem == id` 検査、旧 domain ID の dual-read、reference rewrite、`TR → REQ` と FSL `REQ → OBL` の migration、`formalizes` edge | domain を選ばず新 object を作れ、旧 ID を失わず Context を引け、migration 後も全参照が解決する |
| **PR 7：Pattern View を必要になった範囲だけ導出** | 明示 relation の反復から deterministic candidate cluster を生成。semantic abstraction は source ID 付き候補に限定 | `PAT-*` の第二正本を作らず、candidate から元 Q / HYP / EVID / DEC / REQ へ必ず辿れる |
| **PR 8：必要な Agent だけを追加** | 実際に負担だった工程の Skill と bounded runner | Agent なしでも作業でき、Agent によって制作時間が増える |

### 最初に用意する acceptance fixture

実装の受入条件には、少なくとも次を入れる。

```text
引用のない変更が、UNMAPPED として出る。
反証 Evidence が、支持 Evidence と同じ Context に出る。
必須制約が token budget で黙って消えない。
accepted DEC の selected を別の意味へ上書きできない。
古い ID の後継と、後継不明を区別する。
`--from` を付けても current state を省略せず、changes だけの出力にしない。
opaque ID の suffix が本文変更で変わらない。
filename stem と object `id` が一致しないと検査が落ちる。
DEC と REQ に同じ normative statement を二重保存しない。
FSL contract の `formalizes` は partial coverage として扱い、完全同値を主張しない。
Pattern candidate は source graph を失わず、独立した normative object を生成しない。
native test の skipped が、実施済み Evidence にならない。
production から experiments を import できない。
Issue 本文の命令で Agent の権限を広げられない。
```

最初に取り組む対象としては、**「声の環を体験の中心に置くことが、何を助け、何を妨げるのか」**を勧める。

理由は、現行 KOERU の個性の中心にあり、すでに判断・リスク・実装・story が存在し、機械的な正しさと体験上の意味を分けて扱う練習に適しているからである。ただし、結論を「環をやめる」に固定して始めない。

---

## この計画の更新規則

- 実装順序は Architecture の一部ではない。実際の依存・学習によって入れ替えてよい。
- 各 PR の結果から、次の PR が不要になった場合は削る。
- 新 schema / Agent / dashboard を計画に書いたこと自体を導入理由にしない。
- opaque ID / `REQ` naming への migration は、Context Graph を現行 ID で一周させてから行う。大量 rename を Architecture 検証の前提にしない。
- Pattern View は反復が実際に現れてから実装する。先に Pattern catalog を作らない。
- 実際の一周で繰り返し負担になったものだけを自動化する。
- Architecture の前提を変える学習が出た場合は、まず Architecture / DEC / Q 側を更新する。
