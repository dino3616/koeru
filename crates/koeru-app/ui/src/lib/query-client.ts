import { QueryClient } from "@tanstack/react-query";

/*
 * 問い合わせの設定。
 *
 * ここはローカルのアプリで、相手は同じプロセスの Rust。 ネットワークが
 * 落ちることも、他人が同じデータを書き換えることも無い。だから
 * web 向けの既定（窓に戻るたび取り直す、失敗したら3回試す）は要らない。
 */
export const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        /*
         * 取り直しは無効化でだけ起こす。
         *
         * 台帳が変わる契機はこちらが知っている（テイクの確定、採用の切り替え）。
         * 時間で古くなる類のものではないので、勝手に取り直させない。
         */
        staleTime: Number.POSITIVE_INFINITY,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
        /*
         * 再試行しない。
         *
         * 失敗はディスクや権限の問題で、そのまま3回繰り返しても同じ。
         * 待たせるだけなので、すぐ画面へ出して本人に判断させる。
         */
        retry: false,
      },
    },
  });
