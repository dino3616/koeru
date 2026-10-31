import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { DraftScreen } from "~/screens/draft-screen";

/**
 * 書き出しが済んだ画面。
 *
 * どの書き出しかを検索引数で持つ。 「いちばん新しい書き出し」を読ませると、
 * この画面を開いたまま別の書き出しが済んだとき、見ていたものが黙って入れ替わる。
 */
const searchSchema = v.object({
  release: v.optional(v.string()),
});

export const Route = createFileRoute("/_studio/released")({
  validateSearch: searchSchema,
  component: DraftScreen,
});
