## 何を変えるか

<!-- 1〜3文。何を解決するのか。 -->

## 関連 Issue

<!-- Fixes #123 / Refs #123 -->

## 確認したこと

- [ ] `cargo fmt --all --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] すべてのコミットに `Signed-off-by` がある（`git commit -s`）

## 該当する場合のみ

- [ ] **依存を追加した** → ライセンス種別と一次情報の URL:
- [ ] **学習済みモデル / データセットを追加した** → 学習データの出所とそれぞれのライセンス:
- [ ] **`#[allow(...)]` を追加した** → 理由をコメントに書いた
- [ ] **トレースに新しいフィールドを追加した** → `SENDABLE_FIELDS` を確認し、音源名・パス・歌詞・プロジェクト名を含まないことを確認した
- [ ] **エージェントが生成した PR である** → 音声処理 / DSP / ライセンスに関わる部分を人間が確認した
- [ ] **確定方針に触る変更である** → 先に Issue で合意した（Issue 番号:）
