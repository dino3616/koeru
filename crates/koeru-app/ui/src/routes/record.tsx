import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { RecordScreen } from "~/screens/record-screen";

/**
 * 収録画面が受け取る値。
 *
 * UUID として読めないものは通さない。 読めない識別子で開こうとすると、
 * Rust 側で「そのプロジェクトが無い」になり、原因が遠くなる。
 *
 * `id` そのものは省略できる。 省略を弾くと、殻を先に出しておく段階
 * （prerender）でここを通れず、画面ごとの html が作れない。
 * 無いときの扱いは画面側が持つ。
 */
const searchSchema = v.object({
  id: v.optional(v.pipe(v.string(), v.uuid("プロジェクトの識別子が UUID ではない"))),
});

export const Route = createFileRoute("/record")({
  validateSearch: searchSchema,
  component: RecordScreen,
});
