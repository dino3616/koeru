import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { DraftScreen } from "~/screens/draft-screen";

/**
 * サブバンクのページ。
 *
 * 枠は識別子で指す。 声色・方式・音階の綴りで指さない——付け替えると
 * 綴りは変わるが、識別子は変わらない（`DEC-RCL-018`）。 中身は解釈しないので、
 * UUID かどうかも見ない。
 */
const searchSchema = v.object({
  subbank: v.optional(v.string()),
});

export const Route = createFileRoute("/_studio/subbank")({
  validateSearch: searchSchema,
  component: DraftScreen,
});
