import { QueryClientProvider, QueryErrorResetBoundary } from "@tanstack/react-query";
import type { Preview } from "@storybook/react-vite";
import { Suspense } from "react";

import { CardSkeleton } from "../src/components/card-skeleton";
import { ErrorBoundary } from "../src/components/error-boundary";
import { createQueryClient } from "../src/lib/query-client";

// 相対で書く。 `~` の別名は試験側のサーバへ届かない。
import "../src/styles/globals.css";

/*
 * すべての story に掛かる前提。
 *
 * 配色は明暗の両方で見る（`TR-PLT-25`）。 Radix は light を `:root, .light`、
 * dark を `.dark` に定義するので、`<html>` のクラスで切り替える。
 * 片方だけ見ると、もう片方で 4.5:1 を割っていることに気づけない。
 */
const preview: Preview = {
  parameters: {
    // 違反を見つけたら落とす。報告だけにすると、誰も見ない欄が増える。
    a11y: { test: "error" },
    layout: "centered",
  },
  globalTypes: {
    theme: {
      description: "配色",
      defaultValue: "light",
      toolbar: { icon: "mirror", items: ["light", "dark"], dynamicTitle: true },
    },
  },
  decorators: [
    /*
     * 問い合わせの入れ物・`Suspense`・`ErrorBoundary` を与える。
     *
     * `useSuspenseQuery` を使う部品は、3つとも無いと落ちる。本体の
     * `__root.tsx` と同じ形に揃える。 story ごとに包ませない——
     * 包み忘れた部品が「落ちる story」になって、検査ではなく設置の問題として現れる。
     *
     * 失敗する story はここで実際の失敗画面まで描かれる。 別物の枠で
     * 受けると、本体で出る絵と違うものを検査することになる。
     *
     * client は story ごとに作り直す。 持ち回すと、前の story が入れた
     * 結果を次が読んでしまい、モックを差し替えても絵が変わらない。
     */
    (Story) => (
      <QueryClientProvider client={createQueryClient()}>
        <QueryErrorResetBoundary>
          {({ reset }) => (
            <ErrorBoundary onReset={reset}>
              <Suspense fallback={<CardSkeleton title="読み込み中" />}>
                <Story />
              </Suspense>
            </ErrorBoundary>
          )}
        </QueryErrorResetBoundary>
      </QueryClientProvider>
    ),
    (Story, context) => {
      const dark = context.globals["theme"] === "dark";
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.classList.toggle("light", !dark);
      // 面の上に置く。地の上だけで見ると、段 2 に載る字を一度も検査しない。
      document.body.className = "bg-slate-1 text-slate-12";
      return Story();
    },
  ],
};

export default preview;
