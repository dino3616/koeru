import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { DraftScreen } from "~/screens/draft-screen";

/**
 * 原音設定。
 *
 * 見ているエイリアスは検索引数で持つ。 状態にすると、「録り直す」で収録へ
 * 行って戻ったときに、直していたエイリアスを見失う。
 */
const searchSchema = v.object({
  subbank: v.optional(v.string()),
  entry: v.optional(v.string()),
});

export const Route = createFileRoute("/_studio/review")({
  validateSearch: searchSchema,
  component: DraftScreen,
});
