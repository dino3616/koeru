# QA — Architecture Documentation Bundle

更新日: 2026-09-23

## 実施した検査

この bundle 自体について次を機械検査した。

- Markdown code fence の balance。
- 旧結論（GraphQL 不採用 / Rust 実装から生成した RPC surface を public contract の正本とする方針）の残存検出。
- canonical SDL / Subscription / coherent read / persisted operation の主要 target rule が必要文書に存在すること。
- application-specific Tauri Channel が target architecture として復活していないこと。残存記述は current-state inventory、prohibition、negative test、legacy removal の文脈に限定する。
- MCP/automation consumer が Rust `Application` API を別 public contract として直接利用する設計に戻っていないこと。
- `task-dag.json` の JSON validity と acyclic dependency graph。
- `03-task-dag.md` の T00–T17 に、objective / prerequisites / architecture contract / ownership / must-not-change / tests / observability / migration-recovery / acceptance / adversarial / verification / evidence / human review の13項目が揃うこと。
- Markdown heading の行連結など、編集時の構造破損がないこと。
- `MANIFEST.json` の対象 file size / SHA-256 が現在の bytes と一致すること。

## 実施していないこと

この更新は documentation/design artifact の更新であり、以下を実行したという意味ではない。

- KOERU workspace の `cargo build` / `cargo test` / clippy。
- React/WebView の build/test/Storybook/Playwright。
- GraphQL schema/resolver/codegen の実装・compile test。
- Tauri generic GraphQL Subscription transport の実装検証。
- macOS/Windows/Linux native behavior の確認。
- microphone / playback / realtime audio device verification。
- MFA/alignment/synthesis の real-audio quality verification。
- SQLite migration / crash killpoint / recovery test。
- signing / updater / telemetry provider の M7 release verification。

これらは `03-task-dag.md` と `02-tests-observability.md` に実装時の acceptance/evidence gate として残している。

## 設計上まだ human/product decision が必要な領域

GraphQL 採用によって解決したことに見せてはいけない未決事項として、少なくとも以下を維持する。

- custom recording rules を変更した場合の既存 material の再解釈 semantics。
- retake/new candidate 時の manual edit / confirmation transfer policy。
- ambiguous legacy identity migration の product decision。
- external WAV の normalization/mono/trim policy。
- editor の exact zero-cross / pitchmark behavior と一部 IME/keyboard details。
- alignment/synthesis quality threshold と human acceptance criteria。
- removable/network filesystem を support scope に含める場合の durability policy。
- M7 telemetry SaaS provider、export fields、retention/location。
- model/dataset/license corpus の legal judgment。

## Result

bundle validation が green であることは **文書間の構造・方針整合が取れている**という意味に限定する。KOERU production implementation や native/audio quality が green であることを意味しない。
