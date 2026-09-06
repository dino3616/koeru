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
 */
export const openProjectQuery = (id: string) =>
  queryOptions({
    queryKey: ["project", id],
    queryFn: () => api.openProject(id),
    staleTime: Number.POSITIVE_INFINITY,
  });

/**
 * いまの進み具合。
 *
 * 開いたあとでなければ読めない（開く前に呼ぶと `app.no_project`）。
 * 呼ぶ側は [`openProjectQuery`] を解決した内側に置くこと。
 */
export const progressQuery = () =>
  queryOptions({ queryKey: [LEDGER, "progress"], queryFn: () => api.progress() });

export const songStatusQuery = () =>
  queryOptions({ queryKey: [LEDGER, "songs"], queryFn: () => api.songStatus() });

export const rowsWithTakesQuery = () =>
  queryOptions({ queryKey: [LEDGER, "rows"], queryFn: () => api.rowsWithTakes() });

export const otosQuery = (takeId: number) =>
  queryOptions({ queryKey: [LEDGER, "otos", takeId], queryFn: () => api.otosOfTake(takeId) });

/** デバイスは台帳ではない。抜き差しで変わるが、画面の操作では変わらない。 */
export const devicesQuery = () =>
  queryOptions({ queryKey: ["devices"], queryFn: () => api.listDevices() });

/** 設定値。起動中は変わらない。 */
export const autoAdvanceQuery = () =>
  queryOptions({
    queryKey: ["auto-advance"],
    queryFn: () => api.autoAdvanceMs(),
    staleTime: Number.POSITIVE_INFINITY,
  });

/** 台帳から読むものを全部取り直させる。テイクを確定させたら呼ぶ。 */
export const ledgerKey = [LEDGER] as const;
