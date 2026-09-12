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
  /*
   * 開いた直後に録り直す行（`TR-ALN-27`、`TR-REC-21`）。
   *
   * テイクの面から「もう一度録る」で戻るときに要る。 押した行は
   * `progress.next_row_id` とは限らないので、**行を運ばないと戻った先の
   * 「録る」が別の行を録りはじめる。** 面と同じく検索引数で持つ——
   * 状態にすると、経路をまたいだ時点で消える。
   */
  retake: v.optional(v.string()),
});

export const Route = createFileRoute("/voice")({
  validateSearch: searchSchema,
  component: VoiceScreen,
});
