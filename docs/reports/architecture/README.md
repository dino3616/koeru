# KOERU Architecture Redesign — GraphQL revision

更新日: 2026-09-23

この bundle は、KOERU の現行実装・仕様・meta/evidence・review history を調査して作成した architecture redesign を、GraphQL application contract 採用後の議論に合わせて全面更新したものです。

## 今回確定した設計軸

- `specs/` / `meta/` は引き続き product semantics、invariants、decision の正本。
- canonical `schema.graphql` を **consumer-facing application contract / vocabulary の正本**とする。
- `Rust Domain Model != GraphQL Schema != React UX Model` を明示的な境界とする。
- `Query = coherent application read projection`、`Mutation = user intention / application use case`、`Subscription = long-lived observation / state transition` とする。
- Rust→React の application-visible な継続 stream はすべて GraphQL Subscription に統一する。Tauri `Channel` が残る場合は generic GraphQL execution transport detail のみ。
- React は feature/component の近くに fragment を colocate し、route/workflow boundary が operation を compose する。中央の domain-specific query factory は target architecture では ownership point にしない。
- `ProjectReadSession` により、一つの GraphQL Query 内の project fields は同じ project lease / revision の coherent snapshot から解決する。
- Mutation の expected business outcome、OperationId/idempotency receipt、stale/conflict/recovery semantics は schema 上の第一級 contract とする。通常の business outcome を GraphQL `errors[]` に逃がさない。
- GUI production operation は persisted/approved operation manifest で capability・cost・schema hash と結びつける。
- MCP / automation / extension consumer は同じ canonical GraphQL executor と approved operations を使い、Rust `Application` API を別 public contract として直接公開しない。
- identity、durability、realtime safety、result admission、WASM editing kernel は GraphQL より下位の architecture invariant として維持する。

## 文書一覧

| File | Purpose |
|---|---|
| `00-architecture-report.md` | 調査結果、採用 architecture、Rust/React/application boundary、M6/M7/future consumer を含む主報告書 |
| `01-contributor-guide.md` | placement、dependency、GraphQL contract、streaming、testing、error/observability の contributor rules |
| `02-tests-observability.md` | risk→test→production observability matrix、test cadence、false-green guards、GraphQL/Subscription fitness functions |
| `03-task-dag.md` | GraphQL-first の refactoring task DAG。各 task に objective/contract/tests/evidence/human review を記載 |
| `task-dag.json` | DAG の machine-readable version |
| `04-skills-design.md` | `plan-koeru-change` / `verify-koeru` Agent Skills の更新設計 |
| `05-agent-handoff.md` | refactor 後に Skills を実装する別 agent への standalone handoff |
| `06-evidence.md` | repository evidence と外部技術資料、今回の GraphQL 再判断に使った根拠 |
| `07-worked-examples.md` | recording/editor/import/export/telemetry/Subscription 等の具体例 |
| `08-graphql-application-contract.md` | canonical SDL、Query/Mutation/Subscription、coherent read、transport、evolution、MCP 等の詳細設計 |
| `QA.md` | この documentation bundle に対して実行した整合性検査と未実施事項 |
| `MANIFEST.json` | bundle file hashes / sizes / purpose |
| `validate_bundle.py` | 文書構造・旧結論残存・DAG の機械検査 |

## 読む順序

1. `00-architecture-report.md`
2. `08-graphql-application-contract.md`
3. `01-contributor-guide.md`
4. `02-tests-observability.md`
5. `03-task-dag.md`
6. `07-worked-examples.md`
7. `04-skills-design.md` / `05-agent-handoff.md`
8. `06-evidence.md`

## Status

これは **architecture/research deliverable** です。KOERU repository の production code をこの bundle 内で変更したものではありません。GraphQL schema、resolver、Tauri transport、frontend operations、migration、native/device/audio behavior は target design であり、実装完了の主張ではありません。
