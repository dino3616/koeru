import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { RecordScreen } from "~/screens/record-screen";

/**
 * 収録画面が要る値。
 *
 * **UUID として読めないものは通さない。** 読めない識別子で開こうとすると、
 * Rust 側で「そのプロジェクトが無い」になり、原因が遠くなる。
 */
const searchSchema = v.object({
  id: v.pipe(v.string(), v.uuid("プロジェクトの識別子が UUID ではない")),
});

export const Route = createFileRoute("/record")({
  validateSearch: searchSchema,
  component: RecordScreen,
});
