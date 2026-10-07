---
name: talk-conventions
description: KOERU の発表資料（talks/、dek）を作る・直すときの規約。台本から書く順序、資料を単体で読めるものにすること（リポジトリの文書や ID を引かない）、個人名を出さないこと、見本（dek ref）の読み方、色の規則の置き場所、devShell での動かし方、何をコミットするかを定める。デッキを足す、台本やスライドを書く・直す、テーマを作る、書き出す、発表資料の PR をレビューするときに使う。
---

# KOERU — 発表資料

発表資料は `talks/` の dek のプロジェクトで作る。道具と置き場所の判断は `DEC-PLT-045`。

**dek 自身の規約は写さない。** `talks/AGENTS.md` の dek ブロック（dek が sync のたびに
書き直す）と `bunx dekc help --agent` が正本。ここに書くのは、KOERU の資料として
守ることだけ。

**コマンドは `dekc`。`dek` ではない。** パッケージは `@hajimism/dek` だが、入る
コマンドは `dekc` になる。npm の `dek` は無関係な別のパッケージで、`bunx dek` は
それを取ってきて走らせる。`talks/` の外では `bunx dekc` も同じことが起きるので、
`talks/` に入ってから呼ぶ。

## 置き場所

```text
talks/
├── dek.toml                 # 見本（[refs]）の固定
├── theme.css                # 新しいデッキの出発点
├── decks/<name>/
│   ├── script.md            # 正本。順序・台本・持ち時間
│   ├── theme.css            # このデッキのテーマ。冒頭に色の規則
│   ├── slides/<id>.html     # 1枚1ファイル。任意で .css と .ts
│   └── dist/<name>.html|pdf # 書き出し。コミットする
└── refs/                    # dek ref の実体。gitignore
```

## 順序

1. 台本を書く。`##` が1枚、`###` がビート、段落が喋ること、`>` がト書き
2. `bunx dekc ls` で尺を見る。持ち時間に収まるまで台本を削る
3. 骨格のまま通して喋れるかを見る
4. テーマを作る
5. 1枚ずつ `bunx dekc check <slug> --shot` で確かめ、`bunx dekc shot --sheet` で全体の釣り合いを見る

見た目から始めない。 台本が決まる前に作った枚は、台本が変わると全部作り直しになる。

## 出典

**資料は単体で読めるものにする。** 資料は書き出した HTML と PDF だけが渡り、
リポジトリの外で一人歩きする。スライドにも台本のト書きにも、`docs/` や `meta/` の
文書名・節番号・`DEC-*` `TR-*` などの ID を書かない。読み手が辿れないうえ、
書いておくと資料の外の構成に資料が縛られる。

- 言い換える命題の正しさは、書く前に vision・GTM 計画・meta と突き合わせて確かめる。確かめた跡は資料に残さない
- ジャーニーマップとペルソナは仮説で組んである（`docs/journey-map.md` の前提）。事実のように見せない。出典ではなく、スライドの注に「想定」と書く（`.caveat`）。ト書きに書くのは、何を注で示すかまで

## 書かないこと

- **個人名を出さない。** リポジトリは公開で、資料も誰でも読める。GTM 計画が名前を挙げている人も、資料の中では役割（ツールの作者、初心者の支援者など）で呼ぶ
- 「KOERU が無ければ UTAU は続かない」と言わない（GTM 計画 §4.1）。言えるのは、参加できる人が「エコシステムを自力で統合できる人」に偏る、まで
- まだ動いていないものを、動いているように見せない。今どこまで動くかは `README.md` の状態と `meta/profiles/` が持つ

## 見本の読み方

他人のデッキは `bunx dekc ref <owner/repo/deck>` で固定して読む（`dekc theme <ref>`、`dekc shot <ref> <slug>`）。
固定は `dek.toml` に残り、実体は `talks/refs/` に取り直される。

スライドはコピーしない。 読んで、自分のテーマで書き直す。

見本から取って効いた技術と、作って踏んだことは [references/visual-techniques.md](references/visual-techniques.md) にある。

## 色の規則

デッキの `theme.css` の冒頭に書く。どの色が意味を運び、どこなら装飾してよいかを宣言する。
ここには写さない——デッキごとにテーマを持つので、規則もデッキごとに違う。
規則の立て方と、作り終えてからの突き合わせ方は [references/visual-techniques.md](references/visual-techniques.md) の「色に仕事を持たせる」。

## 動かし方

devShell の中で動かす。bun と Playwright のブラウザは `flake.nix` が出すので、
`playwright install` は要らない（`DEC-PLT-033`）。

```bash
cd talks
bun install
bunx dekc                # 開発サーバ。保存のたびに描画と lint
bun run check            # 型・lint（--visual）・書き出した HTML の鮮度。CI と同じ
bun run build            # HTML と PDF を書き出す
```

**書き出した HTML と PDF はコミットする。** 台本かスライドを変えたら `bun run build` を
走らせて一緒に入れる。HTML が古いと CI が落ちる。PDF は環境で書体と生成日時が
変わるので CI は見ていない——手元で書き出し忘れると、PDF だけが古いまま残る。
