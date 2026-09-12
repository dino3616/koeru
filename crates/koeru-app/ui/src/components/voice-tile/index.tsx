import { VoiceRings } from "~/components/voice-rings";
import type { ProjectView } from "~/lib/ipc";
import { methodLabel } from "~/lib/labels";

type VoiceTileProps = {
  project: ProjectView;
  onOpen: () => void;
};

/**
 * 一覧に並ぶ声1つ（`DEC-PLT-024`、`Q-RCL-004`）。
 *
 * 名前と数字の行に畳まない。 UTAU の音源はキャラクターとして記憶されている
 * （`EVID-UX-004`）。絵を描かせない KOERU では声の形がその位置を引き受けるので、
 * 環をここに出さないと、一覧に何も座らない。
 *
 * 到達度も歌える曲も出す（`TR-RCL-19`）。 中断して数週間後に戻る人が
 * 最初に見るのはこの面で、どれを再開すべきかがここで決まる。
 *
 * 台帳が読めなかった音源も席を残す。 名前だけで並べる——
 * 消すと、壊れていることに気づけないまま音源が消えたように見える。
 */
export const VoiceTile = ({ project, onOpen }: VoiceTileProps) => {
  const state = project.state;
  const pct =
    state !== null && state.required > 0 ? Math.round((state.covered / state.required) * 100) : 0;

  return (
    <button
      type="button"
      onClick={onOpen}
      className="flex size-full flex-col items-center gap-2 rounded-xl border border-transparent p-5 text-slate-12 hover:bg-slate-2"
    >
      {state === null ? (
        <span className="flex size-59 items-center justify-center text-xs text-slate-11">
          中身を読めませんでした
        </span>
      ) : (
        /*
          環に名前を付けない。 すぐ下に同じ数を字で出しているので、
          図にも名前を付けると**的の名前が数を二度読む**——
          「録れた音の形。102 音のうち 14 音 霜月 単独音 14 / 102 音 · 14%」に
          なっていた。図は目で見る人のためだけに置く（`TR-PLT-28` は
          「同じ情報を字でも出す」ことを求めていて、二度読ませることではない）。
        */
        <span className="block size-59 pb-3">
          <VoiceRings rings={state.rings} color={state.color} />
        </span>
      )}

      <span className="text-sm font-semibold">{project.display_name ?? "名前を読めません"}</span>
      <span className="text-xs text-slate-11">{methodLabel(project.method)}</span>
      {/*
        語の途中で折らない（`break-keep`）。 狭い窓では
        **「曲が歌える」が「曲が歌え / る」に割れていた。**
        日本語は語の間に空白が無いので、既定では字数で折られる。
      */}
      {state !== null && (
        <span className="break-keep text-center font-mono text-xs text-slate-11 tabular-nums">
          {state.covered} / {state.required} 音 · {pct}%
          {state.songs_in_bank > 0 &&
            ` · ${state.singable_songs} / ${state.songs_in_bank} 曲が歌える`}
        </span>
      )}
    </button>
  );
};
