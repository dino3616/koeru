import { Chip } from "~/components/chip";
import type { OtoSlot, OtoView } from "~/lib/ipc";
import { usableSpan } from "~/components/take-waveform";

type TakeValuesProps = {
  otos: readonly OtoView[];
  selected: string | null;
  /** 素材の長さ（ミリ秒）。切り出しの終わりを出すのに要る。 */
  durationMs: number;
  /** 生の値と元の名前で見るか。上級者向け（`docs/product-vision.md`）。 */
  raw: boolean;
  onRaw: (raw: boolean) => void;
  /**
   * 人が決めた値（`TR-ALN-30`）。固定は値単位で付く。
   *
   * 「このエントリを触った」では足りない。 オフセットだけ直して残りは
   * 自動のまま、が表せないと、再推定がどちらかを壊す（`INV-ALN-001`）。
   */
  pinned: readonly OtoSlot[];
  /** その値の固定を解いて自動へ戻す（`REQ-ALN-006`）。 */
  onRevert: (slot: OtoSlot) => void;
  busy: boolean;
};

/** 5値の並び。`oto.ini` の並びとは違うので、ここで固定する。 */
const SLOTS: readonly { slot: OtoSlot; label: string; raw: string }[] = [
  { slot: "offset", label: "頭の余白", raw: "offset" },
  { slot: "preutterance", label: "歌い出しの位置", raw: "preutterance" },
  { slot: "overlap", label: "前の音との重なり", raw: "overlap" },
  { slot: "consonant", label: "伸ばさないところ", raw: "consonant" },
  { slot: "cutoff", label: "終わりの余白", raw: "cutoff" },
];

/** その値を、言い換えた側の言い方で。終わりの余白だけ素材の長さから引いて出す。 */
const spoken = (slot: OtoSlot, oto: OtoView, durationMs: number): string => {
  const ms = slot === "cutoff" ? durationMs - usableSpan(oto, durationMs)[1] : oto[`${slot}_ms`];
  return `${(ms / 1000).toFixed(3)} 秒`;
};

/**
 * 1回の録音から取れた音と、その切り出し（`TR-ALN-33`、`TR-EDT-01`）。
 *
 * **言い換えて置く。** エイリアスも oto の語も、既定では出さない
 * （`TR-REC-18`、`docs/reports/ux/direction.md` の「内部表現の名前を出さない」）。
 *
 * ただし本人が生値へ切り替えられる。 上級者が数値を直接触れることは
 * `docs/product-vision.md` が求めている。切り替えは要件に無いので、
 * **判断記録が要る**（この形は `docs/reports/ux/canvas` の付箋に残っている）。
 *
 * 人が決めた値には印を付ける（`TR-ALN-30`）。 印が無いと、再推定で動くものと
 * 動かないものの区別が画面から消える。解く的もその値の隣に置く——
 * まとめて解く的にすると、直した覚えのない値まで巻き戻る。
 *
 * 値をつまんで動かすのは原音設定エディタ（`PROFILE-M6`）。 その描画面の
 * 性質は `Q-PLT-004` が閉じるまで決まらないので、ここは読むことと、
 * 固定を解くことだけを持つ。
 *
 * 音を選ぶ的をここに置かない。 選ぶのは波形の下の帯（`components/take-waveform`）
 * 1箇所だけ。**同じ札を2列並べていたので、押せるのは下の小さいほうだけ、
 * という状態になっていた。踏んだ。** ここは選ばれている音の名前を出すだけ。
 */
export const TakeValues = ({
  otos,
  selected,
  durationMs,
  raw,
  onRaw,
  pinned,
  onRevert,
  busy,
}: TakeValuesProps) => {
  const oto = otos.find((o) => o.alias === selected) ?? otos[0] ?? null;

  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-11">
          {oto === null ? "この行から取れた音" : `波形で選んだ「${oto.alias}」の切り出し`}
        </span>
        <span className="ml-auto">
          <Chip pressed={raw} onClick={() => onRaw(!raw)}>
            数値で見る
          </Chip>
        </span>
      </div>

      {oto === null ? (
        <p className="text-sm text-slate-11">この回からは音が取れませんでした。</p>
      ) : (
        <dl className="grid grid-cols-5 gap-3">
          {SLOTS.map(({ slot, label, raw: rawLabel }) => (
            <div key={slot} className="flex flex-col gap-2">
              <dt className="text-xs text-slate-11">{raw ? rawLabel : label}</dt>
              <dd className="m-0 flex flex-col items-start gap-1">
                <span className="select-text font-mono text-sm text-slate-12 tabular-nums">
                  {raw ? `${oto[`${slot}_ms`].toFixed(3)} ms` : spoken(slot, oto, durationMs)}
                </span>
                {pinned.includes(slot) && (
                  <>
                    <span className="text-xs text-slate-11">手で決めました</span>
                    <button
                      type="button"
                      onClick={() => onRevert(slot)}
                      disabled={busy}
                      className="text-xs text-slate-11 underline hover:text-slate-12 disabled:opacity-45"
                    >
                      自動に戻す
                    </button>
                  </>
                )}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </>
  );
};
