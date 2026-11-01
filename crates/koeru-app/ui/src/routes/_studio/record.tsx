import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { RecordScreen } from "~/screens/record-screen";

/**
 * 収録。 曲を聴いている状態も同じ経路に置く。
 *
 * 聴くのは録る行を離れずにやること。 経路を分けると、聴きはじめた時点で
 * 選んでいた行と枠が検索引数ごと落ち、止めたあと別の行に戻る。
 */
const searchSchema = v.object({
  /** 録る枠。 省くと、全部の枠の録る順の先頭から。 */
  subbank: v.optional(v.string()),
  /*
   * 開いた直後に録る行。
   *
   * 原音設定から「録り直す」で来るときに要る。 押した行は次に録る行とは
   * 限らないので、運ばないと移った先の「録る」が別の行を録りはじめる。
   */
  row: v.optional(v.string()),
});

export const Route = createFileRoute("/_studio/record")({
  validateSearch: searchSchema,
  component: RecordScreen,
});
