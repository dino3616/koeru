# 画面デザインのキャンバス

KOERU の画面デザインを刷新したときの作業面。プロセス（戦略 → 要件 → 構造 → 骨格 → 表層、検証とモデル）と、主要画面を、ボード単位で置いている。

正本ではない。 要件・決定は `meta/` と `specs/` が持ち、ここは読み物と作業記録として置く（[../../README.md](../../README.md)）。ボードの中の数値と文言は、描いた時点の作業の記録で、その後の決定で上書きされうる。

## 中身

| 場所 | 内容 |
|---|---|
| [images/](images/) | 各ボードの書き出し（ボードの幅 × 高さ、1倍） |
| [boards/](boards/) | ボードの HTML（`*.dc.html`）。ブラウザで開ける |
| [canvas.json](canvas.json) | ボードの並び・題・付箋 |
| [model/schema.graphql](model/schema.graphql) | 画面から起こしたオブジェクトとリレーション（GraphQL SDL、構造だけ。Mutation はまだ無い）。図は P13 |
| [delivery-team-plan.md](delivery-team-plan.md) | 画面を差し替えるときの体制と RACI。差し替えが済んだら消す |
| [delivery-phase-plan.md](delivery-phase-plan.md) | 差し替えの工程（段・関門・判定）と画面の順位。差し替えが済んだら消す |

**第三者の画像は入れていない。** 検証に使った市販・配布音源（コル音イム、毒牙ネヲン、足立レイ、重音テト ほか）の立ち絵・アイコン・名前ロゴは権利者の画像なので、それを載せたボード（立ち絵の検証と、実キャラクターでのバリアント）はここに置いていない。本線の画面のうち闇音レンリのアイコンが入る4枚は、アイコンだけを斜線の面（`boards/assets/omitted.svg`）に置き換えた。該当するボードに ※ を付けた。

`boards/support.js` は表示用の最小の代替。 キャンバスの編集機能は持たず、`{{voice}}` などの値を既定値で埋めて描くだけ。フォントは Google Fonts から読む。

## プロセス

| ボード | 画像 | HTML |
|---|---|---|
| 00 表紙 — プロセス全体 | [Main.png](images/Main.png) | [Main.dc.html](boards/Main.dc.html) |
| 01 戦略 | [P1-Strategy.png](images/P1-Strategy.png) | [P1-Strategy.dc.html](boards/P1-Strategy.dc.html) |
| 02 要件 | [P2-Scope.png](images/P2-Scope.png) | [P2-Scope.dc.html](boards/P2-Scope.dc.html) |
| 03 構造 | [P3-Structure.png](images/P3-Structure.png) | [P3-Structure.dc.html](boards/P3-Structure.dc.html) |
| 04 骨格 | [P4-Skeleton.png](images/P4-Skeleton.png) | [P4-Skeleton.dc.html](boards/P4-Skeleton.dc.html) |
| 04+ 骨格 — 新しい音源から収録まで（サブバンク） | [P4b-SkeletonNewVoice.png](images/P4b-SkeletonNewVoice.png) | [P4b-SkeletonNewVoice.dc.html](boards/P4b-SkeletonNewVoice.dc.html) |
| 04+ 骨格 — 音階ピッカー（レーン鍵盤） | [P4c-PitchPicker.png](images/P4c-PitchPicker.png) | [P4c-PitchPicker.dc.html](boards/P4c-PitchPicker.dc.html) |
| 04+ 骨格 — 収録の中で歌う（S05 を S04 に統合） | [P4d-SingInRecord.png](images/P4d-SingInRecord.png) | [P4d-SingInRecord.dc.html](boards/P4d-SingInRecord.dc.html) |
| 04+ 骨格 — 音源のページとサブバンクのページ（S06 を分ける） | [P4e-VoiceAndSubbank.png](images/P4e-VoiceAndSubbank.png) | [P4e-VoiceAndSubbank.dc.html](boards/P4e-VoiceAndSubbank.dc.html) |
| 05+ 収録した音高のグラフ — 調べたことと考察 | [P4f-PitchChart.png](images/P4f-PitchChart.png) | [P4f-PitchChart.dc.html](boards/P4f-PitchChart.dc.html) |
| 04+ 骨格 — 手渡す（S10 · S11 を考え直す） | [P4g-Handoff.png](images/P4g-Handoff.png) | [P4g-Handoff.dc.html](boards/P4g-Handoff.dc.html) |
| 05+ 配る — 調べたことと体験の見直し（1つが基本、増やすと棚） | [P4h-Distribute.png](images/P4h-Distribute.png) | [P4h-Distribute.dc.html](boards/P4h-Distribute.dc.html) |
| 05 表層 — アートディレクション | [P5-Surface.png](images/P5-Surface.png) | [P5-Surface.dc.html](boards/P5-Surface.dc.html) |
| 05 表層 — トークンと部品 | [P6-System.png](images/P6-System.png) | [P6-System.dc.html](boards/P6-System.dc.html) |
| 05+ 敵対的検証 — 存在声域 | [P7-Adversarial.png](images/P7-Adversarial.png) | [P7-Adversarial.dc.html](boards/P7-Adversarial.dc.html) |
| 06 モデル — オブジェクトとリレーション（schema.graphql · 構造） | [P13-Model.png](images/P13-Model.png) | [P13-Model.dc.html](boards/P13-Model.dc.html) |

## 画面

| ボード | 画像 | HTML |
|---|---|---|
| S02 音源の設定 — サブバンクの枠を並べた状態（レーン鍵盤） | [S02-Method.png](images/S02-Method.png) | [S02-Method.dc.html](boards/S02-Method.dc.html) |
| S04 録る | [S04-Record.png](images/S04-Record.png) | [S04-Record.dc.html](boards/S04-Record.dc.html) |
| S04 録る — 曲が歌えるようになった直後（解放の知らせ） | [S04-Unlocked.png](images/S04-Unlocked.png) | [S04-Unlocked.dc.html](boards/S04-Unlocked.dc.html) |
| S04 録る — 歌えるようになった曲を聴いている（⇧Space） | [S04-Listening.png](images/S04-Listening.png) | [S04-Listening.dc.html](boards/S04-Listening.dc.html) |
| S06 音源のページ — ナギ（音域はいつでも触れる・Normal C5 にカーソル） | [S06-Voice.png](images/S06-Voice.png) | [S06-Voice.dc.html](boards/S06-Voice.dc.html) |
| S06b サブバンクのページ — Normal A3 CVVC（担当音域・試聴・音高を重ねる） | [S06b-Subbank.png](images/S06b-Subbank.png) | [S06b-Subbank.dc.html](boards/S06b-Subbank.dc.html) |
| S06b サブバンクのページ — 闇音レンリ（グラフとすべての枠を縦に積む · Normal D4 を重ねている） ※ | [S06b-RenriOverlay.png](images/S06b-RenriOverlay.png) | [S06b-RenriOverlay.dc.html](boards/S06b-RenriOverlay.dc.html) |
| S07 曲バンク — 曲を選び、試唱する範囲をピアノロールで選ぶ（S13 を統合） | [S07-Songs.png](images/S07-Songs.png) | [S07-Songs.dc.html](boards/S07-Songs.dc.html) |
| S08 原音設定 — 要確認を見る | [S08-Review.png](images/S08-Review.png) | [S08-Review.dc.html](boards/S08-Review.dc.html) |
| S10 配布 — すべて · 1.0.1（下書き · バージョンは自分で決める） ※ | [S10-Handoff.png](images/S10-Handoff.png) | [S10-Handoff.dc.html](boards/S10-Handoff.dc.html) |
| S10 配布 — 配布物が4つ · 多音階 Normal 0.9.1（下書き） ※ | [S10-Shelf.png](images/S10-Shelf.png) | [S10-Shelf.dc.html](boards/S10-Shelf.dc.html) |
| S10 配布 — バージョンを選ぶ（⋮ から複製 · 削除） ※ | [S10-Versions.png](images/S10-Versions.png) | [S10-Versions.dc.html](boards/S10-Versions.dc.html) |
| S14 設定 — プライバシー（⌘,） | [S14-Settings.png](images/S14-Settings.png) | [S14-Settings.dc.html](boards/S14-Settings.dc.html) |
| S11 手渡せる状態になった | [S11-Released.png](images/S11-Released.png) | [S11-Released.dc.html](boards/S11-Released.dc.html) |
