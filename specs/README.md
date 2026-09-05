# FSL の正本

形式的な契約はここが正本。 状態、遷移、不変条件、受入条件、禁止条件は FSL が持ち、他の文書は ID で参照する。

FSL に入れないものは3つある。

- 要件そのもの・判断・未決の論点・調査 Evidence・実測予算 → `meta/`（要件は `meta/requirements/`）
- 実時間（ms、パーセンタイル）、確率、連続量、自由文の意味 → FSL の対象外。`meta/budgets/` と実機ベンチが持つ
- ビジョン、ペルソナ、ジャーニーマップ、調査の説明 → `docs/`

## 仕様はスコープで分ける

`manifest` 1つにつき層ごと1仕様なので、スコープが違うものは manifest ごと分ける。

| スコープ | 仕様 | 何を持つか |
|---|---|---|
| 製品全体 | `requirements/telemetry-consent.fsl` | 同意・撤回・送信。プロジェクトが1つも無い時点から存在する |
| プロジェクト1本 | `requirements/project-lifecycle.fsl` | 録音リスト項目ごとの状態、テイクの世代、完成、手渡し |
| （同上・設計層） | `design/project-storage.fsl` | テイク確定を3手に割った実装の輪郭 |
| 収録セッション | `requirements/recording-input.fsl` | デバイス・効果の無効化・校正・入力経路の生死・消失 |
| oto エントリ | `requirements/align-review.fsl` | 確認キュー、人の編集の固定 |
| 編集操作 | `requirements/editor-constraints.fsl` | 制約を破らない編集、通常/上級モードの可逆性 |
| 書き出し | `requirements/packaging-export.fsl` | 検証 → ZIP → 読み戻し、破壊的操作のスナップショット |
| 方式ごとの書き出し可否 | `requirements/method-coverage.fsl` | エイリアス被覆から導出する。上下関係を宣言しない |
| 課題曲 | `requirements/song-coverage.fsl` | 歌える3段階、音高ごとの独立管理 |
| 初回起動 | `requirements/first-run.fsl` | 4操作で最初のフレーズ、マイク権限と拒否からの回復 |
| 課題曲1本の試唱 | `requirements/preview-synthesis.fsl` | 鳴らせるかの判定、短縮版、キャッシュの無効化、中断 |

```
fsl-project.toml          プロジェクトのチェーン（requirements → design → 継ぎ目）
fsl-telemetry.toml        製品全体の同意
fsl-recording-input.toml  収録の入力経路
fsl-align-review.toml     自動原音設定の確認キュー
fsl-preview-synthesis.toml 即時試唱の可否とキャッシュ
refinement/project-design-refines-requirements.fsl   層の継ぎ目
```

スコープを取り違えると、モデルとして誤る。 同意をプロジェクトの状態として持つと、
プロジェクトが2つになった瞬間に同意が複製される。プロジェクトは複数持てる（TR-PKG-37）。

`business` 層はまだ無い。ビジョンの3原則を形式化できるかは未検証で、先に requirements と design の継ぎ目が有効かを確かめる段階にある。

`meta/requirements/` の要件を一度に FSL へ移さないこと。 大半は状態機械ではなく、形式化できない文章まで入れると FSL が新しい巨大文書になる。FSL へ上げるのは、操作の順序や組み合わせで到達してはいけない状態があるものに限る。上げたら、その要件の `formalized_as` に FSL の ID を書く。

## 検証されている契約

| 契約 | どこで |
|---|---|
| 完成は項目の状態だけで決まり、書き出し履歴を参照しない | `INV-PKG-001` |
| 書き出せるのは完成しているときだけ | `INV-PKG-002` |
| 確定したテイクは、削除も上書きもされない | `INV-REC-005` |
| 全テイク無効の項目は、確定したテイクを持たない | `INV-REC-002` |
| 異常終了でも、確定したテイクの数は減らない | `REQ-REC-004` |
| 公開操作なしに完成状態へ到達できる | `REQ-VIS-001` |
| ファイルの無いテイク行が DB にできない | `NoOrphanTakeRow`（設計層） |
| 復旧候補は、本人が採るか捨てるまで消えない | `RecoverableNeverVanishesOnItsOwn` / `OrphanNeverVanishesOnItsOwn`（設計層） |
| 確定済みファイルが減るのは、本人が捨てたときだけ | `FinalizedFilesOnlyLeaveByChoice`（設計層） |
| 残量を見積もらないまま収録を始めない | `FB-REC-107` |
| デバイスを失った状態では収録していない | `INV-REC-101` |
| 入力が届いていないまま収録することはない | `INV-REC-103` |
| 手順の提示は多くとも一度しか出ない | `INV-REC-108` |
| 回り込みを確認しないままガイドを鳴らさない | `INV-REC-105` |
| 鳴らせる長さが足りない曲は試唱に出さない | `INV-SYN-101` |
| キャッシュに載っているのは、解決できるフレーズだけ | `INV-SYN-103` |
| 中断した手は、キャッシュを増やさない | `REQ-SYN-103` |
| 自動の再推定は、固定された値に触れない | `INV-ALN-001` |
| 固定されていることと、人が入れた値であることは同じ | `INV-ALN-002` |
| 確認が残っている間は書き出せない | `INV-ALN-003` |
| 個別確認をやめるのは、確認の上限を超えたときだけ | `INV-ALN-004` |
| 送信が起きるのは、その種別の同意がある間だけ | `INV-TEL-002` |
| 同意を求めるのは、プロジェクト作成と最初の録音を終えたあと | `INV-TEL-003` |

`forbidden` として、同意の撤回後の送信・同意を求める前の送信・計測の同意だけでのクラッシュレポート送信・最初の録音より前に同意を求めること・未完成の書き出し・無効テイクだけでの完成を、いずれも拒否することを検査している。

## 手順

```bash
fslc check   specs/requirements/project-lifecycle.fsl
fslc verify  specs/requirements/project-lifecycle.fsl --depth 12
fslc verify  specs/requirements/project-lifecycle.fsl --engine induction
fslc mutate  specs/requirements/project-lifecycle.fsl --depth 8            # 空洞になっていないか
fslc chain   specs/fsl-project.toml                                     # 全層 + 継ぎ目

# 設計層の帰納法は depth 12 が要る（テイク確定が3手に割れているぶん深い）
fslc verify  specs/design/project-storage.fsl --engine induction --depth 12
```

すべて `proved`。 不変条件は深さの上限なしで成立している。変異検査の kill 率は
project-lifecycle 0.59 / recording-input 0.70 / telemetry-consent 0.66 / project-storage 0.70（深さ8）/
align-review 0.52（深さ6）/ preview-synthesis 0.61 / packaging-export 0.61 /
method-coverage 0.72 / editor-constraints 0.41 / song-coverage 0.54 / first-run 0.70
（いずれも深さ 6〜8）を基準線として扱う。
絶対値ではなく、基準線からの後退が信号。

ゴースト変数は kill 率を下げる。 `align-review` は「直前の手が自動の再推定だったか」を
14 の操作に置いているが、そのうち13は消しても何も壊れない。良性の変異が母数を押し上げるので、
仕様どうしで kill 率を比べない。比べるのは同じ仕様の基準線からの後退だけ。

`fslc` の制約が3つある（4.0.0 時点）。

- `def` は `acceptance` / `forbidden` の中で展開されない。そこだけ式を書き下す
- `relation` を `init` で初期化しないと、空ではなく任意の関係から始まる。空は `Set {}`
- `relation` を持つ仕様は `fslc document generate` が通らない。CI は明示的に除外している

Map を持つ仕様は検証が重い。 設計層と align-review は深さ 8 以下で回すこと。
align-review は帰納法も深さ 8 で回す（深さ 12 では終わらない）。**帰納法が通れば
不変条件は深さの上限なしで成立するので、深さは到達性の witness にしか効かない。**


`fslc` が保証するのは「書かれたモデルの内部整合性」であって、モデルが KOERU の意図を正しく表しているかではない。 そこは人が確かめる。

## 未決を記録する

要件が存在しない箇所に推測で契約を埋めない。`undecided:` で記録し、`meta/questions/` に論点として置く。

```fsl
action crash_and_recover() "undecided: クラッシュ復帰の契約が要件に存在しない" { ... }
```

`undecided:` は検証義務を発生させず、リリースも止めない。 止めたい場合は `meta/questions/*.toml` の `blocks_profiles` に書き、`cargo xtask check-profile` で落とす。
