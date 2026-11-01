import { tv } from "~/lib/tv";

/**
 * 76px の `rec` の丸（S04）。 状態は中の形で言う。 丸（待機）→ 数字（カウント）→
 * 四角（録音中）。 録音中は外に暈を足す。 赤を見分けられない人や白黒の写しでも、
 * 形と字で状態が読める。 中が四角なのはボードとの差（`DEC-PLT-062`）。
 *
 * 赤は録音ボタンだけが持つ（`--color-rec`）。 声の色は主ボタンのものなので使わない。
 *
 * 動かさない。 道具側は動かない（`docs/reports/ux/direction.md`）ので、
 * 状態の切り替えも即座で、`prefers-reduced-motion` で止めるものが無い。
 */
const recordButton = tv({
  variants: {
    state: {
      idle: "",
      countdown: "",
      recording: "shadow-rec-halo",
    },
  },
});

type RecordButtonProps = {
  state: "idle" | "countdown" | "recording";
  /** カウントの残り。 `state` が `countdown` のときだけ読む。 */
  count?: number;
  /** 押したとき。 待機なら始め、カウント中と録音中なら止める。 */
  onPress: () => void;
  /** 確定中など、いま押させない（`TR-REC-42`）。 */
  disabled?: boolean;
};

/**
 * 録音ボタン。 状態は持たず、`use-recorder` が決めた `state` を描く。
 *
 * 見える字は数字だけなので、名前は `aria-label` で言う。 カウント中の名前には
 * 見えている数字を入れる——入れないと axe の `label-content-name-mismatch` が落ちる
 * （見える字が名前に含まれない）。 数字の読み上げは下の常設の領域が持つので、
 * 焦点が当たったままでも名前の書き換わりに頼らない。
 *
 * カウント中の名前は「止める」。 押せば取りやめて、録りはじめない。
 *
 * 領域は常に置き、中身だけを差し替える。 文言と一緒に挿し込むと、
 * 支援技術が変化として拾えず読まれない（`TR-PLT-29`）。
 *
 * キーボードのショートカットを持たない。 素の `<button>` なので、
 * Tab で届き Enter と Space で押せるのは既定のまま。
 */
export const RecordButton = ({ state, count, onPress, disabled = false }: RecordButtonProps) => (
  <>
    <button
      type="button"
      aria-label={
        state === "idle" ? "録る" : state === "countdown" ? `止める カウント ${count}` : "止める"
      }
      disabled={disabled}
      onClick={onPress}
      className={`inline-flex size-19 shrink-0 items-center justify-center rounded-full bg-rec hover:bg-red-12 disabled:pointer-events-none disabled:opacity-45 ${recordButton({ state })}`}
    >
      {state === "idle" && <span className="size-6.5 rounded-full bg-bg" />}
      {state === "countdown" && (
        <span className="font-mono text-xl font-semibold text-slate-1 tabular-nums">{count}</span>
      )}
      {state === "recording" && <span className="size-6.5 rounded-md bg-bg" />}
    </button>
    <p aria-live="polite" aria-atomic="true" className="sr-only">
      {state === "countdown" && count !== undefined ? `${count}` : ""}
    </p>
  </>
);
