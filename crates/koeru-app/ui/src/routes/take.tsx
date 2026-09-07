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
});

export const Route = createFileRoute("/take")({
  validateSearch: searchSchema,
  component: TakeScreen,
});
