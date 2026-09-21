import { queryOptions } from "@tanstack/react-query";

import { api } from "~/lib/ipc";

/*
 * Rust から読むもののクエリ定義。
 *
 * 鍵と取得口を1箇所に置く。 部品の側に散らすと、同じものを別の鍵で引いて
 * 二重に取りに行ったり、無効化したつもりの鍵が実は別物だったりする。
 *
 * ここに置くのは「読み」だけ。 `start_take` や `finish_take` のように
 * 押して初めて走るものは入れない——あれは問い合わせではなく指示で、
 * 取り直しも重複排除も意味を持たない。
 *
 * `revision` を鍵に混ぜない。 台帳が変わったら [`ledgerKey`] で
 * まとめて無効化する。鍵に混ぜると、変わるたびに別の鍵になって
 * キャッシュが積み上がる。
 *
 * # 台帳を読むものは、必ず音源の識別子を鍵に持つ
 *
 * Rust 側は「いま開いている音源」を1つだけ持っていて、`progress()` も
 * `rows_with_takes()` も引数を取らない。だから鍵に識別子を混ぜないと、
 * **どの音源の台帳を読んだのかがキャッシュから消える。**
 *
 * 取り直しは無効化でだけ起こる設定なので（`~/lib/query-client`）、
 * 別の音源を開いても鍵が同じなら前の音源の答えがそのまま出る。
 * **10 音しか録っていない音源を開いて「全部読み終えました」と出た。踏んだ。**
 */

/** 台帳から読むもの。テイクが確定したらまとめて無効になる。 */
const LEDGER = "ledger" as const;

export const projectsQuery = () =>
  queryOptions({ queryKey: [LEDGER, "projects"], queryFn: () => api.listProjects() });

/**
 * プロジェクトを開く。
 *
 * 台帳の鍵の下に置かない。 `open_project` は収録セッションを1つ
 * 始め直す（`TR-REC-30`）ので、テイクが確定するたびに呼ぶと
 * セッションが録音の途中で切り替わる。開くのは画面が出るとき1回だけ。
 *
 * **画面が出るたびに必ず走らせる。** Rust 側は「いま開いている音源」を
 * 1つだけ持っているので、A を開いて B を開いて A へ戻ったとき、
 * ここを飛ばすと **Rust は B を開いたまま A の画面が台帳を読む。**
 * `gcTime: 0` で画面を離れた時点で捨て、次のマウントで必ず取り直す
 * ——これが「開いてから読む」の保証になっている。
 */
export const openProjectQuery = (id: string) =>
  queryOptions({
    queryKey: ["project", id],
    queryFn: () => api.openProject(id),
    staleTime: 0,
    gcTime: 0,
  });

/**
 * いまの進み具合。
 *
 * 開いたあとでなければ読めない（開く前に呼ぶと `app.no_project`）。
 * 呼ぶ側は [`openProjectQuery`] を解決した内側に置くこと。
 */
export const progressQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "progress"], queryFn: () => api.progress() });

/**
 * 開いている音源の環と色（`DEC-PLT-025`、`DEC-PLT-027`）。
 *
 * 台帳の鍵の下に置く。 テイクが確定すると環が伸び、色も動く——
 * 採用テイクの観測から作っているので、切り替えても変わる（`TR-RCL-25`）。
 */
export const voiceStateQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "voice"], queryFn: () => api.voiceState() });

export const songStatusQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "songs"], queryFn: () => api.songStatus() });

/**
 * 取り込んだ曲すべて（`TR-RCL-12`）。曲バンクを組み替える画面が読む。
 *
 * `songStatusQuery` とは別の鍵にする。 あちらはバンクの中だけで、
 * 外した曲が出てこない——同じ鍵に載せると、外した瞬間に戻す的も消える。
 */
export const allSongsQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "all-songs"], queryFn: () => api.allSongs() });

/**
 * その曲を歌うために、あと録る行（`TR-RCL-17`）。
 *
 * 曲ごとに引く。 収録済み単位が増えるたびに変わるので、台帳の鍵の下に置く。
 */
export const songPlanQuery = (id: string, songId: string) =>
  queryOptions({
    queryKey: [LEDGER, id, "song-plan", songId],
    queryFn: () => api.songPlan(songId),
  });

export const rowsWithTakesQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "rows"], queryFn: () => api.rowsWithTakes() });

/** 曲のノート列（`TR-RCL-12`）。範囲を選ぶ画面が読む。 */
export const songNotesQuery = (id: string, songId: string) =>
  queryOptions({
    queryKey: [LEDGER, id, "song-notes", songId],
    queryFn: () => api.songNotes(songId),
  });

/** いまの録る順（`TR-SYN-19`）。台帳が変われば並びも変わる。 */
export const recordingOrderQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "order"], queryFn: () => api.recordingOrder() });

/**
 * 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
 *
 * 台帳の鍵の下に置く。 録るたびに結果が変わる——割れたテイクも
 * 名前も、採用しているテイクから数えている。
 */
export const preflightQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "preflight"], queryFn: () => api.preflight() });

/**
 * 配布に出す値（`PROFILE-M4`）。
 *
 * 台帳の鍵の下に置く。 保存すると変わるので、書いたら無効化する。
 */
export const packageSettingsQuery = (id: string) =>
  queryOptions({
    queryKey: [LEDGER, id, "package-settings"],
    queryFn: () => api.packageSettings(),
  });

/**
 * いま書き出せるか（`TR-PKG-49`）。
 *
 * 台帳の鍵の下に置く。 録るたびにも、設定を書き換えるたびにも変わる。
 * WAV を全部開くので軽くない——取り直しは無効化でだけ起こる。
 */
export const packageStateQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "package-state"], queryFn: () => api.packageState() });

/**
 * 音源アイコンと立ち絵の元画像（`TR-PKG-07`）。
 *
 * 設定と分ける。 設定は打つたびに読み直すが、画像は数 MB あるので、
 * 同じ口に載せると入力のたびに運ぶことになる。
 */
export const packageImageQuery = (id: string, slot: "icon" | "portrait") =>
  queryOptions({
    queryKey: [LEDGER, id, "package-image", slot],
    queryFn: () => (slot === "icon" ? api.packageIcon() : api.packagePortrait()),
  });

/** 配り物に入るもの（`TR-PKG-28` の同梱物）。 */
export const packageContentsQuery = (id: string) =>
  queryOptions({
    queryKey: [LEDGER, id, "package-contents"],
    queryFn: () => api.packageContents(),
  });

/** 書き出しの履歴（`TR-PKG-44`）。書き出すと1つ増える。 */
export const releasesQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "releases"], queryFn: () => api.releases() });

/**
 * そのテイクの原音設定（`TR-ALN-33`）。
 *
 * テイクの識別子は台帳の中で一意なので、音源の識別子を混ぜない。
 * 混ぜても間違いではないが、同じものが音源の数だけ積み上がる。
 */
export const otosQuery = (id: string, takeId: number) =>
  queryOptions({
    queryKey: [LEDGER, id, "otos", takeId],
    queryFn: () => api.otosOfTake(takeId),
  });

/**
 * 確認の進み具合（`TR-ALN-25`）。
 *
 * 台帳の鍵の下に置く。 録るたびにキューが伸び、確認するたびに縮む。
 */
export const reviewSummaryQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "review"], queryFn: () => api.reviewSummary() });

/**
 * 採用テイクのエントリ全部（`TR-ALN-26`）。確認待ちが先。
 *
 * 一覧の絞り込みもここを読む。 どの行が確認待ちかは、
 * エントリの状態からしか分からない（`DEC-PLT-024`）。
 *
 * 確定したものも入っている。 固定は確認が済んだあとも残るので、
 * 確認待ちだけにすると「自動に戻す」が消える。
 */
export const reviewQueueQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "review-queue"], queryFn: () => api.reviewQueue() });

/**
 * モデルが変わって古くなった推定（`TR-ALN-29`）。
 *
 * 台帳の鍵の下に置く。 録り直すと指紋が今のモデルで書き直されるので、
 * テイクが確定するたびに変わる。
 */
export const staleTakesQuery = (id: string) =>
  queryOptions({ queryKey: [LEDGER, id, "stale"], queryFn: () => api.staleTakes() });

/**
 * 同梱しているモデルのライセンス表記（`TR-ALN-31`）。
 *
 * 台帳ではない。 実行ファイルに焼き込んだ台帳から作るので、起動中は変わらない。
 */
export const modelNoticeQuery = () =>
  queryOptions({
    queryKey: ["model-notice"],
    queryFn: () => api.modelNotice(),
    staleTime: Number.POSITIVE_INFINITY,
  });

/**
 * 選べる作り方（`TR-RCL-11`）。
 *
 * 台帳ではない。 録音リストの定義から作るので、起動中は変わらない。
 */
export const methodPresetsQuery = () =>
  queryOptions({
    queryKey: ["methods"],
    queryFn: () => api.methodPresets(),
    staleTime: Number.POSITIVE_INFINITY,
  });

/** デバイスは台帳ではない。抜き差しで変わるが、画面の操作では変わらない。 */
export const devicesQuery = () =>
  queryOptions({ queryKey: ["devices"], queryFn: () => api.listDevices() });

/**
 * この音源で選ばれているマイク（`TR-REC-03`）。
 *
 * 台帳の鍵の下に置かない。 テイクが確定しても選択は変わらないので、
 * 確定ごとに取り直す理由が無い。
 *
 * 音源ごとの鍵にする。 台帳から引く値なので、別の音源へ移ったら別の答えになる。
 */
export const chosenDeviceQuery = (id: string) =>
  queryOptions({
    queryKey: ["chosen-device", id],
    queryFn: () => api.chosenDevice(),
    /*
     * 画面が出るたびに取り直す。
     *
     * **溜めた値を渡さない。** 画面側は初期値としてしか読まない
     * （`useState` に入れる）ので、古い値を返されるとそのまま固定される
     * ——テイクの面から戻ったときに「開いていない」ままになり、
     * 校正と回り込みの確認が押せなくなる。
     */
    staleTime: 0,
    gcTime: 0,
  });

/** 設定値。起動中は変わらない。 */
export const autoAdvanceQuery = () =>
  queryOptions({
    queryKey: ["auto-advance"],
    queryFn: () => api.autoAdvanceMs(),
    staleTime: Number.POSITIVE_INFINITY,
  });

/** 台帳から読むものを全部取り直させる。テイクを確定させたら呼ぶ。 */
export const ledgerKey = [LEDGER] as const;
