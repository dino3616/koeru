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
| [DEC-ALL-007](DEC-ALL-007.toml) | 配布形態との両立 | 署名配布と両立しない動的リンク前提の部品は、実行時 dlopen 経由でのみ到達する | accepted |
| [DEC-ALL-008](DEC-ALL-008.toml) | ライセンスの示し方 | ファイル単位の SPDX ヘッダは置かない。示すのは LICENSE と Cargo.toml だけ | accepted |
| [DEC-ALL-009](DEC-ALL-009.toml) | レビュー基盤 | ReviewGraphen は今は採らない。設計を読む対象として残す | accepted |
| [DEC-ALL-010](DEC-ALL-010.toml) | 構造化の基盤 | HigherGraphen は採らない。触るなら上に載る ReviewGraphen の側 | accepted |
| [DEC-ALN-001](DEC-ALN-001.toml) |  | 強制アライメントは Julius セグメンテーションキットを一次経路にする | accepted |
| [DEC-ALN-002](DEC-ALN-002.toml) | 原音設定 | 原音設定は自動を既定とし、setParam / vLabeler と同等の編集機能を必ず持つ | accepted |
| [DEC-ALN-003](DEC-ALN-003.toml) | 確認の上限 | 確認待ちの上限を件数ではなく合計所要時間で切り、通常モードは合計5分とする | accepted |
| [DEC-ALN-004](DEC-ALN-004.toml) | アライナの音響モデル | Julius セグメンテーションキットの .binhmm を、判断で通す | accepted |
| [DEC-ALN-005](DEC-ALN-005.toml) | 録音品質の判定 | 録音品質の判定は確認時に行い、録り直しの提案までに留める | accepted |
| [DEC-ALN-006](DEC-ALN-006.toml) | 単独音の境界検出 | 単独音も Julius を一次経路とし、いまのヒューリスティックは退避に置く | superseded |
| [DEC-ALN-007](DEC-ALN-007.toml) | 到達水準の判定時期 | M3 では評価ハーネスを持たない。到達水準の判定は M6 へ回す | accepted |
| [DEC-ALN-008](DEC-ALN-008.toml) | 強制アライメントの一次経路 | MFA 日本語音響モデルを同梱し、Kaldi を Rust から直接叩く。上流コーパスの条件は判断で通す | accepted |
| [DEC-ALN-009](DEC-ALN-009.toml) | 原音設定の置き場所 | koeru-align を作って原音設定を集約し、アライナは trait で切る | accepted |
| [DEC-ALN-010](DEC-ALN-010.toml) | 退避経路の段数 | Julius の実装は M5 へ送る。M3 の退避は segment.rs 1段にする | superseded |
| [DEC-ALN-011](DEC-ALN-011.toml) | 方式別の規約をいつ書くか | CVVC の VC 規約と多音階の扱いを M5 へ送る | accepted |
| [DEC-ALN-012](DEC-ALN-012.toml) | モデルの同梱方法 | MFA のモデルを HuggingFace の submodule で同梱し、v3.3.0 へ上げる | accepted |
| [DEC-ALN-013](DEC-ALN-013.toml) | 1ファイルに入るモーラ数 | 単独音も1ファイルに複数モーラが入る。oto はモーラごとに持つ | accepted |
| [DEC-ALN-014](DEC-ALN-014.toml) | oto の再導出の担当 | アライメント境界を保存し、下位方式の5値の再導出を align に置く | accepted |
| [DEC-ALN-015](DEC-ALN-015.toml) | 退避経路の段数 | Julius は採らない。退避は segment.rs の1段に確定し、多モーラ方式では MFA を必須にする | accepted |
| [DEC-ALN-016](DEC-ALN-016.toml) | 退避経路の段数 | アライメントの退避経路を持たない。MFA が無ければ自動原音設定を行わない | accepted |
| [DEC-ALN-017](DEC-ALN-017.toml) | 原音設定エントリの同一性 | 原音設定エントリの同一性を（収録音高, 綴り）にし、同じ音高の中で綴りの持ち主を1行に決める | superseded |
| [DEC-ALN-018](DEC-ALN-018.toml) | 無声破裂音の分岐の事後検証 | 無声破裂音の閉鎖は直前の母音と比べて検証し、閾値は実測までの仮置きにする | accepted |
| [DEC-EDT-003](DEC-EDT-003.toml) | 違反と確認済み | 上級モードの自動確認済みに「制約違反が残っていない場合に限る」を課す | accepted |
| [DEC-PKG-001](DEC-PKG-001.toml) | 完成 | 完成状態と手渡し状態を直交させる | accepted |
| [DEC-PKG-002](DEC-PKG-002.toml) | 周波数表 | `.frq` は録音時に作る | accepted |
| [DEC-PKG-003](DEC-PKG-003.toml) | 手渡し | 手渡しは配布パッケージの生成までとし、配布の場や発見機能は持たない | accepted |
| [DEC-PKG-004](DEC-PKG-004.toml) | ファイル名 | 書き出すファイル名は ASCII 固定とし、読み込みは日本語にも対応する | accepted |
| [DEC-PKG-005](DEC-PKG-005.toml) | 方式の関係 | 方式の上下関係を宣言せず、エイリアス被覆から導出する | accepted |
| [DEC-PKG-006](DEC-PKG-006.toml) | プロジェクトの実体 | プロジェクトを UUID 名のディレクトリにし、人間可読な manifest を平文で添える | accepted |
| [DEC-PKG-007](DEC-PKG-007.toml) | 完成の関門 | 完成の条件を先に満たさせる。名前は必須にし、あとから変えられるようにする | accepted |
| [DEC-PKG-008](DEC-PKG-008.toml) | 配布名 | 配布名を表示名から分け、書き出しのときに決めさせる | accepted |
| [DEC-PKG-009](DEC-PKG-009.toml) | エントリ名の符号化 | エントリ名を ASCII に固定し、EFS フラグを立てない | accepted |
| [DEC-PKG-010](DEC-PKG-010.toml) | アーカイブの形 | UAR を ZIP と併せて出す | accepted |
| [DEC-PKG-011](DEC-PKG-011.toml) | 利用規約 | 利用規約の設問を持たず、自由記述にする | accepted |
| [DEC-PKG-012](DEC-PKG-012.toml) | 音源アイコン | 音源アイコンは本人の画像から作る | accepted |
| [DEC-PKG-013](DEC-PKG-013.toml) | 配り物への到達 | 配り物の置き場所を、OS のファイルマネージャで見せる | accepted |
| [DEC-PKG-014](DEC-PKG-014.toml) | 下位方式の出どころ | 下位方式へ降りるとき、綴りごとに行頭のモーラを持つ素材から採る | accepted |
| [DEC-PKG-015](DEC-PKG-015.toml) | 配布物の方式の名乗り | readme の収録方式と収録音高を別の節に分け、収録音高は単音階でも出す | accepted |
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
| [DEC-PLT-017](DEC-PLT-017.toml) | 画面へ流し続けるものの経路 | 流し続けるものは Channel で送る。画面から引きに行かせない | superseded |
| [DEC-PLT-018](DEC-PLT-018.toml) | メモ化の置き場所 | React Compiler を通す。手でメモ化しない | accepted |
| [DEC-PLT-019](DEC-PLT-019.toml) | 画面と Rust の型の一致 | tauri-specta で画面の型と呼び出し口を Rust から生成する。rspc は採らない | superseded |
| [DEC-PLT-020](DEC-PLT-020.toml) | クラス名の組み立て | tailwind-variants を variants にだけ使う。tailwind-merge は使わず、className を props で受けない | superseded |
| [DEC-PLT-021](DEC-PLT-021.toml) | 画面の分け方 | OOUI はカード（面）の単位で採る。ルートはオブジェクトごとに分けない | accepted |
| [DEC-PLT-022](DEC-PLT-022.toml) | アクセシビリティの自動検査 | Storybook の story を検査範囲にし、axe を実ブラウザで当てる。自前の配色検査は廃止する | accepted |
| [DEC-PLT-023](DEC-PLT-023.toml) | 読みの取り回し | Rust からの読みは TanStack Query に載せ、Suspense と ErrorBoundary で受ける | superseded |
| [DEC-PLT-024](DEC-PLT-024.toml) | 画面の骨格 | 面を工程で切らず、オブジェクトで切る。詳細の主語はテイク、音高は第一級の軸 | accepted |
| [DEC-PLT-025](DEC-PLT-025.toml) | 画面の重心と視覚言語 | 中央に「育っていく声」を置く。声から決定的に形を生成し、ロゴも同じ規則から作る | accepted |
| [DEC-PLT-026](DEC-PLT-026.toml) | 同梱する日本語書体 | Noto Sans JP を1本だけ同梱する。カバレッジを個性より優先し、サブセット化しない | accepted |
| [DEC-PLT-027](DEC-PLT-027.toml) | 声から色を導く規則 | 音高帯で色相の区画を選び、重心が区画の中の位置と彩度を決める。彩度の上限は色相ごとに sRGB から出す | accepted |
| [DEC-PLT-028](DEC-PLT-028.toml) | 画面で色相を持ってよいもの | 押せるものの塗りとフォーカス環を無彩色へ移す。cyan と jade を画面から外す | accepted |
| [DEC-PLT-029](DEC-PLT-029.toml) | 画面が成立する窓の範囲 | 3列に下限を持たせて 1200px で段を切る。マイクの選択は Rust が持つ | accepted |
| [DEC-PLT-030](DEC-PLT-030.toml) | クラス名の畳み | 畳むのは外から来たものとの突き合わせ1箇所だけ。`tv` は畳まない入口から取り、`cn` で後勝ちにする | accepted |
| [DEC-PLT-031](DEC-PLT-031.toml) | デザインシステムの規則の検査 | デザインシステムの規則は `@shadcn/lint` を oxlint から呼んで見る。`components.json` を置く | accepted |
| [DEC-PLT-032](DEC-PLT-032.toml) | 画面遷移の持ち場 | 画面遷移は Motion の AnimateView が持ち、CSS には root の抑止だけ残す | accepted |
| [DEC-PLT-033](DEC-PLT-033.toml) | 開発環境の調達 | 開発ツールは Nix の devShell で揃える。Nix を第一級の前提開発環境とする | accepted |
| [DEC-PLT-034](DEC-PLT-034.toml) | アーキテクチャの境界 | 意味を決める所と確定する所を分け、寿命ごとの実行域に載せる | accepted |
| [DEC-PLT-035](DEC-PLT-035.toml) | 画面と Rust の契約の正本 | consumer への契約は specs/application/schema.graphql を正本にし、process 内で実行する。Rust の型から契約を生成しない | accepted |
| [DEC-PLT-036](DEC-PLT-036.toml) | 画面へ流し続けるものの経路 | 画面へ流し続けるものは GraphQL Subscription にする。用途ごとの Channel を契約にしない | accepted |
| [DEC-PLT-037](DEC-PLT-037.toml) | 読みの取り回し | 画面が読むものは部品の近くの fragment で宣言し、経路が1つの operation に束ねる。結果の寿命は TanStack Query が持つ | accepted |
| [DEC-PLT-038](DEC-PLT-038.toml) | 失敗の分類と伝え方 | 失敗は「呼び出し側の次の手」と「確定したかどうか」で分類する。予期できる結果は失敗にせず、失敗の記録は持ち主が1回だけ型で出す | accepted |
| [DEC-PLT-039](DEC-PLT-039.toml) | 検査が通ったことの意味 | テストは危険ごとに最も安い検出点を選び、必須の suite は meta に登録して実行した件数で判定する。前提を欠いたら黙って return しない | accepted |
| [DEC-RCL-001](DEC-RCL-001.toml) | 方式選択 | 方式は最初に選ばせ、選択肢は「手作業が必要かどうか」を主軸に見せる | accepted |
| [DEC-RCL-002](DEC-RCL-002.toml) | 方式変換 | 方式変換は上位から下位への書き出しだけを見込み、逆は採らない | accepted |
| [DEC-RCL-003](DEC-RCL-003.toml) | 進捗と課題曲 | カバレッジと歌える曲を常時両方見せ、曲は入口としてだけ使う | accepted |
| [DEC-RCL-004](DEC-RCL-004.toml) | 収録単位の数 | 収録単位の数を presamp からの導出結果に合わせ、141/168 という数字を捨てる | accepted |
| [DEC-RCL-005](DEC-RCL-005.toml) | 辞書の同梱 | 歌詞の g2p を M2 から外す。UST は仮名を持っているので、主経路は g2p 無しで通る | superseded |
| [DEC-RCL-006](DEC-RCL-006.toml) | 音源の一覧が持つ情報 | 一覧にも到達度を出す。音源ごとに台帳を開いて、環と色と数を作る | accepted |
| [DEC-RCL-007](DEC-RCL-007.toml) | 下位方式への移行 | 下位方式への書き出しを用意し、声質は検知せず素材の由来を提示する | superseded |
| [DEC-RCL-008](DEC-RCL-008.toml) | 所要時間の根拠 | 行読み上げの所要時間を 1行 12.0 秒で確定し、実測は残り時間の側だけに効かせる | accepted |
| [DEC-RCL-009](DEC-RCL-009.toml) | 生成できる条文であること | 連続音・CVVC の条文の数と制約を、導出結果とグラフの性質に合わせる | accepted |
| [DEC-RCL-010](DEC-RCL-010.toml) | 歌詞の g2p | 歌詞の g2p を落とす。要件ごと削除し、辞書は同梱しない | accepted |
| [DEC-RCL-011](DEC-RCL-011.toml) | 曲先行の録音リスト | 選んだノート群から録音リストを詰め直す。部分集合に限らない | accepted |
| [DEC-RCL-012](DEC-RCL-012.toml) | USTX の取り込み | USTX は yaml_serde で読む。YAML パーサを自前で書かない | accepted |
| [DEC-RCL-013](DEC-RCL-013.toml) | 残り時間の出どころ | 所要時間を実測で推定しない。固定の見積もり1本にし、残りは件数と併記する | accepted |
| [DEC-RCL-014](DEC-RCL-014.toml) | 方式と収録音高の分け方 | 収録音高をプリセットから外し、本数も音高も本人に選ばせる | accepted |
| [DEC-RCL-015](DEC-RCL-015.toml) | 下位方式への移行 | 下位方式の書き出しから素材の由来を落とす。声質の推測材料も置かない | accepted |
| [DEC-RCL-016](DEC-RCL-016.toml) | 一度録った綴りを二度読ませない | 未収録の行を録った綴りに合わせて組み直し、重なった綴りは先に録った行が持つ | accepted |
| [DEC-REC-001](DEC-REC-001.toml) | 音声 I/O | 音声 I/O は各 OS の API を直接叩く。抽象レイヤを挟まない | accepted |
| [DEC-REC-002](DEC-REC-002.toml) | 録音条件 | 録音条件は、ある程度の品質のマイクと通常の声量を前提にする | accepted |
| [DEC-REC-003](DEC-REC-003.toml) | 前処理 | 録音直後のオフライン前処理を、試唱と配布に同じく適用する | accepted |
| [DEC-REC-004](DEC-REC-004.toml) | クラッシュ復帰 | テイクの永続化を、ファイル確定が DB コミットに先行する順序で行う | accepted |
| [DEC-REC-005](DEC-REC-005.toml) | FSL の有界化を写さない | 検証用の有界化（MAX_TAKES）は写さない。テイク数に上限は無い | accepted |
| [DEC-REC-006](DEC-REC-006.toml) | マスターのサンプルレート | キャプチャからマスターへの変換を pump に置く。レートを下流へ持ち回さない | accepted |
| [DEC-REC-007](DEC-REC-007.toml) | リングバッファの位置の持ち方 | リングの位置は総数で持つ。剰余で持たない | accepted |
| [DEC-REC-008](DEC-REC-008.toml) | 録音の観測をどこまで出すか | 機材と設定の不備も指摘しない。観測をフラットに報告するだけにする | accepted |
| [DEC-REC-009](DEC-REC-009.toml) | 入力レベルの見せ方 | 入力レベルから区分ごと外す。行 ID は画面のどこにも出さない | accepted |
| [DEC-SYN-001](DEC-SYN-001.toml) | 合成 | 合成は WORLD ベースとし、F0 推定のみ SwiftF0 に差し替える | accepted |
| [DEC-SYN-002](DEC-SYN-002.toml) | 中核体験 | 録音の途中でも自分の声で歌を聴けることを中核体験に置く | accepted |
| [DEC-SYN-003](DEC-SYN-003.toml) | 外部エンジン | 既定は同梱コア。本人がローカルに持つ resampler を指して使えるようにする | accepted |
| [DEC-SYN-004](DEC-SYN-004.toml) | F0 推定 | SwiftF0 を採用する。重みは MIT で明示されており、著者の表示を額面どおり受け取る | accepted |
| [DEC-SYN-005](DEC-SYN-005.toml) | resampler | UTAU 互換 resampler を WORLD の上に自前で書く | accepted |
| [DEC-SYN-006](DEC-SYN-006.toml) | WORLD の取り込み方 | WORLD の C++ をリポジトリへ同梱し、cc でビルドする | superseded |
| [DEC-SYN-007](DEC-SYN-007.toml) | F0 推定の同梱 | M2 では Harvest のまま進む。SwiftF0 は実測で差が聞こえてから入れる | accepted |
| [DEC-SYN-008](DEC-SYN-008.toml) | 周波数表の呼び出し規約 | 周波数表はファイル全体・.frq の格子で渡し、切り出しは合成器がする | accepted |
| [DEC-SYN-009](DEC-SYN-009.toml) | フレーズの拍と音符の対応 | 長音と促音も拍として返す。長音は直前母音を伸ばす | accepted |
| [DEC-SYN-010](DEC-SYN-010.toml) | phonemizer の差し替え | phonemizer の差し替えを presamp.ini に置き、音素の時間位置は KOERU の規約に残す | accepted |
| [DEC-SYN-011](DEC-SYN-011.toml) | CVVC の候補順 | CVVC の候補順にも直前の音符を見せる | accepted |
| [DEC-SYN-012](DEC-SYN-012.toml) | 曲のキーを誰が決めるか | 自動移調をやめる。キーは本人が決め、KOERU は勧めるだけにする | accepted |
| [DEC-SYN-013](DEC-SYN-013.toml) | 綴りの表の固定 | presamp.ini はプロジェクトを作るときに選ばせて固定し、あとからの書き換えは戻して別名で残す | accepted |
| [DEC-SYN-014](DEC-SYN-014.toml) | 多音階の解決順 | 多音階ではノートごとに使える収録音高の中で解き、同じ高さの中の代用を下の高さより先に試す | accepted |
| [DEC-TEL-001](DEC-TEL-001.toml) | 利用計測 | 利用計測は既定オフのオプトインとし、SaaS 経由でホワイトリスト送信する | accepted |

123 件。
