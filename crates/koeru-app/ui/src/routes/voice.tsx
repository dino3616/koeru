import { createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { VoiceScreen } from "~/screens/voice-screen";

/**
 * 音源の面が受け取る値。
 *
 * UUID として読めないものは通さない。 読めない識別子で開こうとすると、
 * Rust 側で「そのプロジェクトが無い」になり、原因が遠くなる。
 *
 * `id` そのものは省略できる。 省略を弾くと、殻を先に出しておく段階
 * （prerender）でここを通れず、画面ごとの html が作れない。
 * 無いときの扱いは画面側が持つ。
 *
 * 面は検索引数で持つ。 状態にすると、戻ってきたときに必ず「音」へ戻る——
 * 曲を見ていた人が、テイクを開いて戻ると別の面にいることになる。
 */
const searchSchema = v.object({
  id: v.optional(v.pipe(v.string(), v.uuid("音源の識別子が UUID ではない"))),
  tab: v.optional(v.picklist(["sound", "songs", "package", "settings"], "その面は無い"), "sound"),
});

export const Route = createFileRoute("/voice")({
  validateSearch: searchSchema,
  component: VoiceScreen,
});
