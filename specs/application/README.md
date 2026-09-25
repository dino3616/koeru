# consumer への契約（canonical SDL）

KOERU が画面と、これから来る consumer（MCP・自動化・拡張）に約束する application の言葉。
正本であることと、process 内で実行することは `DEC-PLT-035` が決めている。
設計の説明は [08-graphql-application-contract.md](../../docs/reports/architecture/08-graphql-application-contract.md)。

| ファイル | 何か |
|---|---|
| `schema/*.graphql` | 契約そのもの。 領域ごとのファイルに分け、すべてで1つの契約になる。 読みの Query、意図の Mutation、観測の Subscription |
| `operations/*.graphql` | 領域ごとの見本の operation と fragment。 画面の実装ではない |
| `capabilities.toml` | 欄ごとの能力と費用の区分。 operation の manifest がここから能力を足し合わせる |

## 何を持ち、何を持たないか

SDL が持つのは、consumer から見える語彙・形・能力・予期できる結果。
状態・遷移・不変条件は FSL（[../README.md](../README.md)）、要件と判断は `meta/` が持つ。
SDL の description は規則を写さずに ID で引く。 FSL と食い違ったら FSL が正。

- 領域は `Project` の面（`recording` / `review` / `repertoire` / `editor` / `distribution` / `jobs`）に分け、
  同じ版から読む。 画面の名前は型にも欄にも出さない
- 大きいもの（波形・スペクトログラム・取り込む前のファイル）は root の窓の欄に置く。
  一覧は `WindowInput` で窓を切り、上限は `ApplicationInfo.limits` が返す
- 予期できる結果は union で返す（`DEC-PLT-038`）。 失敗の形は interface `Problem` にそろえる
- 識別子は種類ごとに scalar を分け、エイリアス・表示文・パスを鍵にしない（`DEC-RCL-017`）
- 流し続けるものは Subscription で、届け方は持ち主ごとに違う（`DEC-PLT-036`）

Rust の実装が SDL を満たすかは T09 が runtime の SDL との一致で見る。 画面の operation と
生成する型は T11 が見る。 ここにあるのは、実装が無くても検査できる部分だけ。

## 変え方

規則は `DEC-PLT-035` の「変更の規則」にある。 ここでは写さない。

`schema/` はファイルを領域（面）ごとに分ける。 領域をまたいで使う語彙（識別子と量の scalar、
`Problem` と共通の結果、`WindowInput`）は `shared.graphql`、root の型の本体は `project.graphql` が持ち、
ほかの領域は自分のファイルで `extend type Query` / `Mutation` / `Subscription` に欄を足す。
検査はファイル名の順にすべてを読み、1つの契約として組む。

`@deprecated` には代わりに使うものを書く。 見本の operation は非推奨の欄を選ばない
（どちらも `check-schema` が落とす）。 main と PR のあいだで壊れる変更を見つけるのは
GraphQL Inspector の役目で（`DEC-PLT-040`）、npm の部品を入れる段で足す。

## 検査

```bash
cargo xtask check-schema
```

Rust の実装も画面も組み立てずに、ここの3つだけを読む。 GraphQL の仕様どおりかは
apollo-compiler が見て、その上に次を足す。 どれも根拠を `xtask/src/schema.rs` の doc に書いてある。

| 規則 | 落とすもの |
|---|---|
| 名前の形 | PascalCase でない型、camelCase でない欄、SCREAMING_SNAKE_CASE でない enum の値、`Input` の付け方の不揃い |
| パス | 名前が `path` / `dir` / `directory` / `folder` で終わる欄・引数 |
| 識別子 | `id` / `…Id` を組み込みの scalar にしたもの、どこかで組み込みの `ID` を使ったもの、`key` / `…Key` を文字列にしたもの |
| root | null を返す root の欄、union を返さない Subscription |
| Mutation の形 | `x(input: XInput!): XPayload!` と `outcome: XOutcome!`（union）から外れたもの。 `operationId` と受領証の片方だけを持つもの |
| 古くなりうる入力 | 貸与・版・操作の識別子・続きの位置・仕事の識別子を取るのに、それが古いときの結果を union に持たないもの |
| 環 | 欄をたどって自分へ戻る出力の型 |
| 非推奨 | reason の無い `@deprecated`、非推奨の欄を選ぶ見本 |
| operation | 名前の無い operation、`持ち主_型` でない fragment、見本が1本も無いこと |
| 能力の表 | 能力を持たない root の欄、SDL に無い座標、語彙に無い能力と費用、使われない語彙 |

見本が選んでいない root の欄は、落とさずに数と名前を出す。

## 観測に載せてよいもの

operation の名前・正規化した文書の hash・種類・契約の hash だけ。 文書の本文と変数は
通常のトレースにも送信にも載せない（`DEC-PLT-009`）。 変数にはパス・歌詞・エイリアス・
題が入りうる。 契約の hash の作り方は operation の manifest と一緒に T10 で決める。

## 旧コマンドからの移行

tauri-specta の 78 のコマンド（`crates/koeru-app/src/lib.rs` の登録）が、どこへ行くか。
新しい操作を tauri-specta に足さない（`DEC-PLT-035`）。

| コマンド | 行き先 | 種類 |
|---|---|---|
| `list_devices` | `Query.audio` の `inputDevices` | Query |
| `list_projects` | `Query.library` | Query |
| `method_presets` | `Query.catalog` の `methodPresets` | Query |
| `tone_suggestions` | `Query.catalog` の `toneSuggestions` | Query |
| `tone_options` | `Query.catalog` の `toneOptions` | Query |
| `create_project` | `Mutation.createProject` | Mutation |
| `rename_project` | `Mutation.renameProject` | Mutation |
| `voice_state` | `Project.voice` | Query |
| `open_project` | `Mutation.openProject`。 進み具合は返った貸与で `Query.project` から読む | Mutation |
| `presamp_notice` | `Project.notices` の `RulesFileRestored` | Query |
| `dismiss_presamp_notice` | `Mutation.dismissProjectNotice` | Mutation |
| `progress` | `Project.voice` と `RecordingFacet` の `remaining` / `byTone` / `order` | Query |
| `chosen_device` | `RecordingFacet` の `chosenDevice` / `input` / `activeCapture` | Query |
| `arm_device` | `Mutation.armInput` | Mutation |
| `probe_input` | `Mutation.probeInput` | Mutation |
| `start_take` | `Mutation.startTake`（`row` を省く） | Mutation |
| `start_retake` | `Mutation.startTake`（`row` を指す） | Mutation |
| `rows_with_takes` | `RecordingFacet.rows` | Query |
| `recording_order` | `RecordingFacet.order` | Query |
| `set_recording_order` | `Mutation.chooseRecordingOrder` | Mutation |
| `adopt_take` | `Mutation.adoptTake` | Mutation |
| `otos_of_take` | `Take.entries` | Query |
| `play_take` | `Mutation.playTake` と `Subscription.playbackEvents` | Mutation |
| `stream_envelope` | `Subscription.inputEnvelope` | Subscription |
| `stop_envelope_stream` | 無くなる。 観測をやめるのは transport の unsubscribe（T10） | 廃止 |
| `finish_take` | `Mutation.finishTake`。 解析の進みは `Take.analysis` と `Subscription.jobEvents` | Mutation |
| `preview` | `Mutation.auditionTake` | Mutation |
| `preroll_ms` | `InputEnvelopeFrame.prerollMs`。 テイクごとは `CaptureReport.prerollMs` | Subscription |
| `estimate_space` | `RecordingFacet.space` | Query |
| `calibrate` | `Mutation.calibrateInput` | Mutation |
| `gain_drift` | `InputLease.gainDrift` | Query |
| `restore_saved_gain` | `Mutation.restoreSavedGain` | Mutation |
| `auto_advance_ms` | `RecordingFacet.autoAdvanceMs` | Query |
| `output_kind` | `Query.audio` の `outputRoute` | Query |
| `check_guide_leak` | `Mutation.checkGuideLeak`。 最後の結果は `InputLease.lastGuideLeakCheck` | Mutation |
| `play_pitch` | `Mutation.playGuidePitch` | Mutation |
| `song_status` | `RepertoireFacet.bank` | Query |
| `song_plan` | `Song.plan` | Query |
| `song_file_preview` | `Query.songFileDrafts` | Query |
| `import_songs` | `Mutation.importSongs` | Mutation |
| `rename_song` | `Mutation.renameSong` | Mutation |
| `all_songs` | `RepertoireFacet.songs` | Query |
| `set_song_transpose` | `Mutation.transposeSong` | Mutation |
| `set_song_in_bank` | `Mutation.addSongToBank` / `Mutation.removeSongFromBank` | Mutation |
| `repack_for_selection` | `Mutation.repackRecordingList`。 範囲は配列の位置ではなく `NoteId` で指す | Mutation |
| `song_notes` | `Song.notes` | Query |
| `sing_song` | `Mutation.singSong` と `Subscription.playbackEvents` | Mutation |
| `pending_work` | `Subscription.jobQueue`。 今の数は `Project.jobs` | Subscription |
| `latency_report` | `RepertoireFacet.previewLatency` | Query |
| `waveform_window` | `Query.waveformWindow` | Query |
| `spectrogram_window` | `Query.spectrogramWindow` | Query |
| `preflight` | `ExportReadiness.materials` と `Mutation.prepareExport`。 名前を NFC へ直す副作用は計画の側へ移す（T08） | Query |
| `use_mixed_channels` | `Mutation.mixInputChannels` | Mutation |
| `stop_preview` | `Mutation.stopPlayback` | Mutation |
| `review_summary` | `ReviewFacet` の `mode` / `summary` | Query |
| `review_queue` | `ReviewFacet.queue` | Query |
| `confirm_entry` | `Mutation.confirmEntry` | Mutation |
| `confirm_all_entries` | `Mutation.confirmPendingEntries` | Mutation |
| `switch_review_mode` | `Mutation.switchReviewMode` | Mutation |
| `edit_oto_value` | `Mutation.moveBoundary`（`gesture: TYPED_VALUE`） | Mutation |
| `revert_oto_value` | `Mutation.revertBoundary` | Mutation |
| `rerecord_entry` | `Mutation.requestRetake` | Mutation |
| `validate_otos` | `Mutation.validateEntries` | Mutation |
| `export_otos` | `Mutation.exportSettingsFile`。 パスを返さない | Mutation |
| `stale_takes` | `ReviewFacet.outdatedRows` | Query |
| `model_notice` | `ApplicationInfo.bundledModelNotice` | Query |
| `package_settings` | `DistributionFacet.settings` | Query |
| `set_package_settings` | `Mutation.saveDistributionSettings` | Mutation |
| `set_package_icon` | `Mutation.replaceDistributionImage` / `removeDistributionImage`（`ICON`） | Mutation |
| `set_package_portrait` | `Mutation.replaceDistributionImage` / `removeDistributionImage`（`PORTRAIT`） | Mutation |
| `package_icon` | `DistributionFacet.icon` の `content` | Query |
| `package_portrait` | `DistributionFacet.portrait` の `content` | Query |
| `package_state` | `DistributionFacet.readiness` | Query |
| `package_contents` | `DistributionFacet.contents` | Query |
| `export_package` | `Mutation.prepareExport` → `Mutation.commitExport` | Mutation |
| `export_downgrade` | `Mutation.prepareExport`（`method` を指す） → `Mutation.commitExport` | Mutation |
| `releases` | `DistributionFacet.releases` | Query |
| `reveal_release` | desktop host の API。 渡すのは `ReleaseId` だけで、在り処は host が引く（`TR-PKG-45`） | host |

コマンドの外で消えるもの。 `finish_take` が返していた波形のサムネイルは
`Query.waveformWindow` を小さい画素数で読む。 行と確認の `key`（結合した文字列）は
`RowId` / `EntryId` / `TargetId` に分かれる。

## 語彙の判断

語彙の選び方は判断記録が持つ。 ここでは場所だけを示す。

| 場所 | 判断 |
|---|---|
| 識別子の scalar、`ByteCount` / `SampleCount` / `SamplePosition`、`Base64Bytes`、`FileInput.name`、`capabilities.toml`、面の名前と説明の言語、互換の窓 | `DEC-PLT-042` |
| `EntryBoundaries`、`ConfirmEntryInput` と境界の編集の `EditingSessionId` | `DEC-EDT-004` |

まだ決めていないものは次の段で決める。 一括編集の残りの種類の入力の形（`BulkEditInput`）は T13、
書き出し前の NFC 化を書き出しの計画だけで行うか（`preflight` の副作用）は T08。
