# 判断記録の索引

         `schema = 'decision'` のファイルの一覧。この索引は手で書かない。
         `cargo xtask index-decisions` が `meta/decisions/*.toml` から作る。
         中身を直すのは各 TOML 側で、索引は作り直す。

         読み方と規律は [../README.md](../README.md)。置き換えの関係（`supersedes` /
         `superseded_by` / `status = 'superseded'`）は `cargo xtask check-meta` が双方向で検査する。

         | ID | 何についての判断か | 決めたこと | 状態 |
         |---|---|---|---|
| [DEC-ALL-001](DEC-ALL-001.toml) | 学習済みモデルの許諾 | 配布モデルの再配布許諾が確認できない部品は採らない | accepted |
| [DEC-ALL-002](DEC-ALL-002.toml) | ライセンス不明・非商用 | ライセンス不明または非商用限定の部品は採らない | accepted |
| [DEC-ALL-003](DEC-ALL-003.toml) | コピーレフトと代替 | コピーレフトの部品は、同等の代替があるなら代替を採る | accepted |
| [DEC-ALL-004](DEC-ALL-004.toml) | 商用購入が前提の基盤 | 商用ライセンス購入が前提の GUI・音声フレームワークは採らない | accepted |
| [DEC-ALL-005](DEC-ALL-005.toml) | 既存ツールの参照方法 | コピーレフトの既存ツールは、公開された機能的事実からのみ互換基準を定める | accepted |
| [DEC-ALL-006](DEC-ALL-006.toml) | ローカル完結と単一実装 | ローカル完結・単一実装の方針に反する部品は採らない | accepted |
| [DEC-ALL-007](DEC-ALL-007.toml) | ライセンスの示し方 | ファイル単位の SPDX ヘッダは置かない。示すのは LICENSE と Cargo.toml だけ | accepted |
| [DEC-ALN-001](DEC-ALN-001.toml) |  | 強制アライメントは Julius セグメンテーションキットを一次経路にする | accepted |
| [DEC-ALN-002](DEC-ALN-002.toml) | 原音設定 | 原音設定は自動を既定とし、setParam / vLabeler と同等の編集機能を必ず持つ | accepted |
| [DEC-ALN-003](DEC-ALN-003.toml) | 確認の上限 | 確認待ちの上限を件数ではなく合計所要時間で切り、通常モードは合計5分とする | accepted |
| [DEC-ALN-004](DEC-ALN-004.toml) | アライナの音響モデル | Julius セグメンテーションキットの .binhmm を、判断で通す | accepted |
| [DEC-ALN-005](DEC-ALN-005.toml) | 録音品質の判定 | 録音品質の判定は確認時に行い、録り直しの提案までに留める | accepted |
| [DEC-ALN-006](DEC-ALN-006.toml) | 単独音の境界検出 | 単独音も Julius を一次経路とし、いまのヒューリスティックは退避に置く | accepted |
| [DEC-ALN-007](DEC-ALN-007.toml) | 到達水準の判定時期 | M3 では評価ハーネスを持たない。到達水準の判定は M6 へ回す | accepted |
| [DEC-ALN-008](DEC-ALN-008.toml) | 強制アライメントの一次経路 | MFA 日本語音響モデルを同梱し、Kaldi を Rust から直接叩く。上流コーパスの条件は判断で通す | accepted |
| [DEC-ALN-009](DEC-ALN-009.toml) | 原音設定の置き場所 | koeru-align を作って原音設定を集約し、アライナは trait で切る | accepted |
| [DEC-ALN-010](DEC-ALN-010.toml) | 退避経路の段数 | Julius の実装は M5 へ送る。M3 の退避は segment.rs 1段にする | accepted |
| [DEC-ALN-011](DEC-ALN-011.toml) | 方式別の規約をいつ書くか | CVVC の VC 規約と多音階の扱いを M5 へ送る | accepted |
| [DEC-ALN-012](DEC-ALN-012.toml) | モデルの同梱方法 | MFA のモデルを HuggingFace の submodule で同梱し、v3.3.0 へ上げる | accepted |
| [DEC-ALN-013](DEC-ALN-013.toml) | 1ファイルに入るモーラ数 | 単独音も1ファイルに複数モーラが入る。oto はモーラごとに持つ | accepted |
| [DEC-EDT-003](DEC-EDT-003.toml) | 違反と確認済み | 上級モードの自動確認済みに「制約違反が残っていない場合に限る」を課す | accepted |
| [DEC-PKG-001](DEC-PKG-001.toml) | 完成 | 完成状態と手渡し状態を直交させる | accepted |
| [DEC-PKG-002](DEC-PKG-002.toml) | 周波数表 | `.frq` は録音時に作る | accepted |
| [DEC-PKG-003](DEC-PKG-003.toml) | 手渡し | 手渡しは配布パッケージの生成までとし、配布の場や発見機能は持たない | accepted |
| [DEC-PKG-004](DEC-PKG-004.toml) | ファイル名 | 書き出すファイル名は ASCII 固定とし、読み込みは日本語にも対応する | accepted |
| [DEC-PKG-005](DEC-PKG-005.toml) | 方式の関係 | 方式の上下関係を宣言せず、エイリアス被覆から導出する | accepted |
| [DEC-PKG-006](DEC-PKG-006.toml) | プロジェクトの実体 | プロジェクトを UUID 名のディレクトリにし、人間可読な manifest を平文で添える | accepted |
| [DEC-PLT-001](DEC-PLT-001.toml) | 形態 | 実装スタックを Rust + Tauri にする | accepted |
| [DEC-PLT-002](DEC-PLT-002.toml) | ライセンス | ライセンスを AGPL-3.0-or-later にする | accepted |
| [DEC-PLT-003](DEC-PLT-003.toml) | 配布 | 配布は直接ダウンロードを主経路とし、Microsoft Store を取らない | accepted |
| [DEC-PLT-004](DEC-PLT-004.toml) |  | Linux は Flatpak の1形態で配布する | accepted |
| [DEC-PLT-005](DEC-PLT-005.toml) | データ | 処理はローカル完結とし、声をサーバへ送らない | accepted |
| [DEC-PLT-006](DEC-PLT-006.toml) | 対象 | まず UTAU エコシステムを対象とし、ニューラル音源データセット系は初期サポート外 | accepted |
| [DEC-PLT-007](DEC-PLT-007.toml) | アクセシビリティ | アクセシビリティを既定要件とする | accepted |
| [DEC-PLT-008](DEC-PLT-008.toml) | スコープ外 | 録音品質のリアルタイム判定・プロトタイプ検証フェーズ・UI の多言語対応をスコープ外にする | accepted |
| [DEC-PLT-009](DEC-PLT-009.toml) | 出力の経路 | 出力は tracing に一本化し、println! / eprintln! / dbg! を使わない | accepted |
| [DEC-PLT-010](DEC-PLT-010.toml) | 予算行の担当 | 予算の空行に担当領域と目標値を入れ、絶対値が置けないものはアクセスパターンで縛る | accepted |
| [DEC-PLT-011](DEC-PLT-011.toml) | OpenUtau との関係 | OpenUtau のコードは取り込まず、仕様として参照し、互換は CI で検証する | accepted |
| [DEC-PLT-012](DEC-PLT-012.toml) | メモリ予算 | メモリ予算を領域ごとの独立キャップから、実行モード別の単一予算の配分に変える | accepted |
| [DEC-PLT-013](DEC-PLT-013.toml) | 文字符号化 | CP932 は encoding_rs を採る。個人アカウントだが実質の保守主体は Mozilla | accepted |
| [DEC-PLT-014](DEC-PLT-014.toml) | 描画面 | 描画は Canvas 2D と WebGL2 に置く。WebGPU（vgpu）は採らない | accepted |
| [DEC-PLT-015](DEC-PLT-015.toml) | フロントの枠組み | React + TanStack Start（SPA）+ Tailwind + Radix。shadcn は写す先であって依存先ではない | accepted |
| [DEC-PLT-016](DEC-PLT-016.toml) | C / C++ の調達 | C / C++ は submodule で調達する。WORLD の同梱も submodule へ移す | accepted |
| [DEC-PLT-017](DEC-PLT-017.toml) | 画面へ流し続けるものの経路 | 流し続けるものは Channel で送る。画面から引きに行かせない | accepted |
| [DEC-PLT-018](DEC-PLT-018.toml) | メモ化の置き場所 | React Compiler を通す。手でメモ化しない | accepted |
| [DEC-PLT-019](DEC-PLT-019.toml) | 画面と Rust の型の一致 | tauri-specta で画面の型と呼び出し口を Rust から生成する。rspc は採らない | accepted |
| [DEC-PLT-020](DEC-PLT-020.toml) | クラス名の組み立て | tailwind-variants を variants にだけ使う。tailwind-merge は使わず、className を props で受けない | accepted |
| [DEC-PLT-021](DEC-PLT-021.toml) | 画面の分け方 | OOUI はカード（面）の単位で採る。ルートはオブジェクトごとに分けない | accepted |
| [DEC-PLT-022](DEC-PLT-022.toml) | アクセシビリティの自動検査 | Storybook の story を検査範囲にし、axe を実ブラウザで当てる。自前の配色検査は廃止する | accepted |
| [DEC-RCL-001](DEC-RCL-001.toml) | 方式選択 | 方式は最初に選ばせ、選択肢は「手作業が必要かどうか」を主軸に見せる | accepted |
| [DEC-RCL-002](DEC-RCL-002.toml) | 方式変換 | 方式変換は上位から下位への書き出しだけを見込み、逆は採らない | accepted |
| [DEC-RCL-003](DEC-RCL-003.toml) | 進捗と課題曲 | カバレッジと歌える曲を常時両方見せ、曲は入口としてだけ使う | accepted |
| [DEC-RCL-004](DEC-RCL-004.toml) | 収録単位の数 | 収録単位の数を presamp からの導出結果に合わせ、141/168 という数字を捨てる | accepted |
| [DEC-RCL-005](DEC-RCL-005.toml) | 辞書の同梱 | 歌詞の g2p を M2 から外す。UST は仮名を持っているので、主経路は g2p 無しで通る | accepted |
| [DEC-REC-001](DEC-REC-001.toml) | 音声 I/O | 音声 I/O は各 OS の API を直接叩く。抽象レイヤを挟まない | accepted |
| [DEC-REC-002](DEC-REC-002.toml) | 録音条件 | 録音条件は、ある程度の品質のマイクと通常の声量を前提にする | accepted |
| [DEC-REC-003](DEC-REC-003.toml) | 前処理 | 録音直後のオフライン前処理を、試唱と配布に同じく適用する | accepted |
| [DEC-REC-004](DEC-REC-004.toml) | クラッシュ復帰 | テイクの永続化を、ファイル確定が DB コミットに先行する順序で行う | accepted |
| [DEC-REC-005](DEC-REC-005.toml) | FSL の有界化を写さない | 検証用の有界化（MAX_TAKES）は写さない。テイク数に上限は無い | accepted |
| [DEC-REC-006](DEC-REC-006.toml) | マスターのサンプルレート | キャプチャからマスターへの変換を pump に置く。レートを下流へ持ち回さない | accepted |
| [DEC-REC-007](DEC-REC-007.toml) | リングバッファの位置の持ち方 | リングの位置は総数で持つ。剰余で持たない | accepted |
| [DEC-SYN-001](DEC-SYN-001.toml) | 合成 | 合成は WORLD ベースとし、F0 推定のみ SwiftF0 に差し替える | accepted |
| [DEC-SYN-002](DEC-SYN-002.toml) | 中核体験 | 録音の途中でも自分の声で歌を聴けることを中核体験に置く | accepted |
| [DEC-SYN-003](DEC-SYN-003.toml) | 外部エンジン | 既定は同梱コア。本人がローカルに持つ resampler を指して使えるようにする | accepted |
| [DEC-SYN-004](DEC-SYN-004.toml) | F0 推定 | SwiftF0 を採用する。重みは MIT で明示されており、著者の表示を額面どおり受け取る | accepted |
| [DEC-SYN-005](DEC-SYN-005.toml) | resampler | UTAU 互換 resampler を WORLD の上に自前で書く | accepted |
| [DEC-SYN-006](DEC-SYN-006.toml) | WORLD の取り込み方 | WORLD の C++ をリポジトリへ同梱し、cc でビルドする | superseded |
| [DEC-SYN-007](DEC-SYN-007.toml) | F0 推定の同梱 | M2 では Harvest のまま進む。SwiftF0 は実測で差が聞こえてから入れる | accepted |
| [DEC-SYN-008](DEC-SYN-008.toml) | 周波数表の呼び出し規約 | 周波数表はファイル全体・.frq の格子で渡し、切り出しは合成器がする | accepted |
| [DEC-SYN-009](DEC-SYN-009.toml) | フレーズの拍と音符の対応 | 長音と促音も拍として返す。長音は直前母音を伸ばす | accepted |
| [DEC-TEL-001](DEC-TEL-001.toml) | 利用計測 | 利用計測は既定オフのオプトインとし、SaaS 経由でホワイトリスト送信する | accepted |

71 件。
