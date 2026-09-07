import { Breath } from "~/components/breath";
import { Button } from "~/components/button";
import { VoiceRings } from "~/components/voice-rings";
import type { VoiceStateView } from "~/lib/ipc";

type VoicePortraitProps = {
  state: VoiceStateView;
  /** 中央を押したとき。押すたびに歌う（`DEC-PLT-025`）。 */
  onListen: () => void;
  /** 鳴っている音を止める。 */
  onStop: () => void;
  /** 声を用意しているあいだ。押してから鳴るまでを無言にしない（`TR-SYN-33`）。 */
  preparing: boolean;
  /** 環を伸ばす。テイクが確定した直後だけ。 */
  grow?: boolean;
};

/**
 * 育っていく声。音源の面の中央に常駐する（`DEC-PLT-025`）。
 *
 * 中央の「聴く」はいつでも押せる。 1音も録れていなくても押せて、
 * そのときは鳴らないことが返事になる——押せなくすると、
 * 何が足りないのかが分からないまま的が消える。
 *
 * 数はいつも環と一緒に出す。 カバレッジと「いま歌える曲の数」の
 * どちらも隠さない（`TR-RCL-19`）。環は色だけの図なので、
 * 数を添えないと画面を見ていない人に何も届かない（`TR-PLT-28`）。
 *
 * 評価しない。 測った値をそのまま出す（`TR-SYN-20`、`DEC-REC-008`）。
 * 「あと〇〇%」も「よくできました」も書かない。
 *
 * 環は余った高さに合わせて縮む。 一辺を固定にすると、下に置く「次に読む」が
 * 画面の外へ出る——**既定の窓（1280×840）で「録る」が下端で切れていた。**
 * 押せない理由を書いた1行はさらにその下なので、灰色の的だけが見えていた。
 * **踏んだ。** 縮む下限は、内側の環が「聴く」に重ならない大きさ
 * （`components/voice-rings` の `INNER_R`）。
 */
export const VoicePortrait = ({
  state,
  onListen,
  onStop,
  preparing,
  grow = false,
}: VoicePortraitProps) => {
  const pct = state.required > 0 ? Math.round((state.covered / state.required) * 100) : 0;

  return (
    <div className="flex min-h-0 flex-col items-center gap-3">
      {/*
        高さは2つの制約で決まる。 列の余りに合わせて縮む（`shrink`）ぶんと、
        窓の高さに対する上限（`max-h-[40vh]`）。**窓の高さ側を持たないと、
        縦に積む狭い窓で環が最大のまま「次に読む」を押し出す。**
        画面側の段（1200px）をここへ写さないために、vh で持つ。
      */}
      <div className="relative aspect-square h-75 max-h-[40vh] min-h-60 shrink">
        <VoiceRings
          rings={state.rings}
          color={state.color}
          grow={grow}
          label={
            state.covered === 0
              ? "まだ録っていません"
              : `録れた音の形。${state.required} 音のうち ${state.covered} 音`
          }
        />
        <button
          type="button"
          onClick={onListen}
          aria-busy={preparing}
          className="-translate-x-1/2 -translate-y-1/2 absolute top-1/2 left-1/2 size-19 rounded-full bg-slate-11 text-sm font-semibold text-slate-1 hover:bg-slate-12"
        >
          聴く
        </button>
      </div>

      <p className="font-mono text-sm text-slate-12 tabular-nums">
        {state.covered} / {state.required} 音 · {pct}%
      </p>
      {state.songs_in_bank > 0 && (
        <p className="font-mono text-sm text-slate-12 tabular-nums">
          {state.singable_songs} / {state.songs_in_bank} 曲が歌える
        </p>
      )}

      {/*
        止める的は、鳴っていなくても置いておく。
        鳴っているあいだだけ出すと、曲が終わる瞬間に的が消えて押し損ねる。
        鳴っていないときに押しても何も起きない。
      */}
      <Button variant="ghost" size="sm" onClick={onStop}>
        止める
      </Button>

      {/*
        待ちを無言にしない（`TR-SYN-33`、`docs/design/direction.md`）。
        領域は常に置き、中身だけを差し替える——文言と一緒に挿し込むと、
        支援技術が変化として拾えない。
      */}
      <p aria-live="polite" aria-atomic="true" className="text-xs text-slate-11">
        {preparing ? "声をつくっています" : ""}
      </p>
      {preparing && <Breath />}
    </div>
  );
};
