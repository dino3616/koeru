import type { RowStatus } from "~/components/status-mark";
import type { RowTakesView, SongView } from "~/lib/ipc";

import type { SidebarRow, SidebarSong, SidebarSongState } from ".";

/**
 * `SongView` から曲の状態を決める（`TR-RCL-19`）。
 *
 * 「あと N 行」と「音域外」はどちらも `Unavailable` で届く。 見分けは `rescuing_tone`:
 * 足すと今のキーのまま届く音高があれば、欠けているのは行数ではなく音域。
 * 知らない語は `Unavailable` として扱う——歌えると言い切らない側へ倒す。
 */
export const songState = (
  view: Pick<SongView, "singability" | "rescuing_tone" | "missing_rows">,
): SidebarSongState => {
  if (view.singability === "Complete") return { status: "ready" };
  if (view.singability === "WithFallback") return { status: "fallback" };
  if (view.rescuing_tone !== null) return { status: "out_of_range" };
  return { status: "remaining", rows: view.missing_rows };
};

/** 音域は `SongView` に無い。 画面が持っているときだけ渡す。 */
export const songItem = (view: SongView, range: string | null = null): SidebarSong => ({
  id: view.id,
  title: view.title,
  range,
  state: songState(view),
});

const ROW_STATUSES: readonly RowStatus[] = ["unrecorded", "recorded", "needs_retake", "excluded"];

/**
 * `RowTakesView.state` を行の状態へ絞る。 bindings は `string` で渡すので、
 * 知らない語は `unrecorded` に倒す（契約が列挙になれば要らなくなる、`DEC-PLT-035`）。
 */
export const rowStatus = (state: string): RowStatus =>
  ROW_STATUSES.find((s) => s === state) ?? "unrecorded";

export const rowItem = (view: Pick<RowTakesView, "row_id" | "text" | "state">): SidebarRow => ({
  id: view.row_id,
  text: view.text,
  status: rowStatus(view.state),
});
