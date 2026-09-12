import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { TakeScreen } from "~/screens/take-screen";

/**
 * 録った回の面が受け取る値。
 *
 * 行を指す手段が要る。 同じ読み上げ文字列の行が複数ありうるので、
 * テキストでは指せない（`Q-REC-003`）。**画面には出さない**——
 * 経路の引数として持つだけで、内部表現を面に出さない（`TR-REC-18`）。
 *
 * `id` は音源。ここから直接開かれる経路があるので（`TR-PKG-51`、`TR-ALN-27`）、
 * 音源の面を経由しない入口も成り立たせる。
 */
const searchSchema = v.object({
  id: v.optional(v.pipe(v.string(), v.uuid("音源の識別子が UUID ではない"))),
  row: v.optional(v.string()),
  /*
   * どの面から来たか。戻るときにそこへ返す。
   *
   * 既定は「音」。 音源の面を経由しない入口（`TR-PKG-51`、`TR-ALN-27`）では
   * 戻り先が無いので、収録の面へ返す。
   *
   * **持たないと、配り物から開いたテイクが音の面へ戻る。** 面を検索引数に
   * 置いたのは「戻ってきたときに別の面にいない」ためなので、ここで落とすと
   * その意図が半分しか効かない。
   */
  from: v.optional(v.picklist(["sound", "songs", "package", "settings"], "その面は無い")),
});

export const Route = createFileRoute("/take")({
  validateSearch: searchSchema,
  component: TakeScreen,
});
