# FSL の正本

**形式的な契約はここが正本。** 状態、遷移、不変条件、受入条件、禁止条件は FSL が持ち、他の文書は ID で参照する。

FSL に入れないものは3つある。

- **要件そのもの・判断・未決の論点・調査 Evidence・実測予算** → `meta/`（261件の要件は `meta/requirements/`）
- **実時間（ms、パーセンタイル）、確率、連続量、自由文の意味** → FSL の対象外。`meta/targets/` と実機ベンチが持つ
- **ビジョン、ペルソナ、ジャーニーマップ、調査の説明** → `docs/`

## 仕様はスコープで分ける

**`manifest` 1つにつき層ごと1仕様**なので、スコープが違うものは manifest ごと分ける。

| スコープ | 仕様 | 何を持つか |
|---|---|---|
| **製品全体** | `requirements/telemetry-consent.fsl` | 同意・撤回・送信。プロジェクトが1つも無い時点から存在する |
| **プロジェクト1本** | `requirements/project-lifecycle.fsl` | カバレッジ・完成・手渡し |
| （同上・設計層） | `design/project-storage.fsl` | テイク確定を3手に割った実装の輪郭 |

```
fsl-project.toml      プロジェクトのチェーン（requirements → design → 継ぎ目）
fsl-telemetry.toml    製品全体の同意
refinement/project-design-refines-requirements.fsl   層の継ぎ目
```

**スコープを取り違えると、モデルとして誤る。** 同意をプロジェクトの状態として持つと、
プロジェクトが2つになった瞬間に同意が複製される。プロジェクトは複数持てる（TR-PKG-37）。

`business` 層はまだ無い。ビジョンの3原則を形式化できるかは未検証で、先に requirements と design の継ぎ目が有効かを確かめる段階にある。

**`meta/requirements/` の261件を一度に FSL へ移さないこと。** 大半は状態機械ではなく、形式化できない文章まで入れると FSL が新しい巨大文書になる。FSL へ上げるのは、**操作の順序や組み合わせで到達してはいけない状態がある**ものに限る。上げたら、その要件の `formalized_as` に FSL の ID を書く。

## 検証されている契約

| 契約 | どこで |
|---|---|
| 完成判定は coverage のみで決まり、書き出し履歴を参照しない | `INV-PKG-001` |
| 書き出せるのは完成しているときだけ | `INV-PKG-002` |
| 無効テイクはカバレッジに加算されない | `INV-REC-001` |
| 送信が起きるのは、その種別の同意がある間だけ | `INV-TEL-002` |
| 同意を求めるのは、プロジェクト作成と最初の録音を終えたあと | `INV-TEL-003` |
| 異常終了で失われるのは進行中のテイクだけ | `REQ-REC-004` |
| 公開操作なしに完成状態へ到達できる | `REQ-VIS-001` |
| 必要な本数が揃う前でも試唱できる | `REQ-SYN-001` |
| ファイルの無いテイク行が DB にできない | `NoOrphanTakeRow`（設計層） |

`forbidden` として、同意の撤回後の送信・同意を求める前の送信・**計測の同意だけでのクラッシュレポート送信**・最初の録音より前に同意を求めること・未完成の書き出し・無効テイクだけでの完成を、いずれも拒否することを検査している。

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

**両層とも `proved`。** 不変条件は深さの上限なしで成立している。変異検査の kill 率は要求層 0.73 / 設計層 0.54 で、これを基準線として扱う。**絶対値ではなく、基準線からの後退が信号。**

```bash
```

**`fslc` が保証するのは「書かれたモデルの内部整合性」であって、モデルが KOERU の意図を正しく表しているかではない。** そこは人が確かめる。

## 未決を記録する

要件が存在しない箇所に推測で契約を埋めない。`undecided:` で記録し、`meta/questions/` に論点として置く。

```fsl
action crash_and_recover() "undecided: クラッシュ復帰の契約が要件に存在しない" { ... }
```

**`undecided:` は検証義務を発生させず、リリースも止めない。** 止めたい場合は `meta/questions/*.toml` の `blocks_profiles` に書き、`cargo xtask check-profile` で落とす。
