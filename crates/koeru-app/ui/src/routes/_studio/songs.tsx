import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { DraftScreen } from "~/screens/draft-screen";

/**
 * 曲バンク。
 *
 * 選んでいる曲は検索引数で持つ。 状態にすると、収録へ行って戻ったときに
 * 一覧の先頭へ戻る。
 */
const searchSchema = v.object({
  song: v.optional(v.string()),
});

export const Route = createFileRoute("/_studio/songs")({
  validateSearch: searchSchema,
  component: DraftScreen,
});
