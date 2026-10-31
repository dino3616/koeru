import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { DraftScreen } from "~/screens/draft-screen";

/**
 * アプリの設定。
 *
 * 収録の準備（デバイス・校正・回り込みの確認）は `recording` のタブに置く。
 * 録る画面に出すのはデバイスの選択だけにする——校正と回り込みの確認は
 * 毎回やるものではなく、録る行と声だけを残す画面に席を取らせない。
 * マイクの選択は音源ごとなので、`id` が無いとこのタブは中身を出せない。
 */
const searchSchema = v.object({
  tab: v.optional(
    v.picklist(["general", "recording", "export", "tools", "display", "privacy"], "その面は無い"),
    "general",
  ),
});

export const Route = createFileRoute("/_studio/settings")({
  validateSearch: searchSchema,
  component: DraftScreen,
});
