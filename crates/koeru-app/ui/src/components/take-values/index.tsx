import { Chip } from "~/components/chip";
import type { OtoView } from "~/lib/ipc";
import { usableSpan } from "~/components/take-waveform";

type TakeValuesProps = {
  otos: readonly OtoView[];
  selected: string | null;
  /** 素材の長さ（ミリ秒）。切り出しの終わりを出すのに要る。 */
  durationMs: number;
  /** 生の値と元の名前で見るか。上級者向け（`docs/product-vision.md`）。 */
  raw: boolean;
  onRaw: (raw: boolean) => void;
};

/**
 * 1回の録音から取れた音と、その切り出し（`TR-ALN-33`、`TR-EDT-01`）。
 *
 * **言い換えて置く。** エイリアスも oto の語も、既定では出さない
 * （`TR-REC-18`、`docs/design/direction.md` の「内部表現の名前を出さない」）。
 *
 * ただし本人が生値へ切り替えられる。 上級者が数値を直接触れることは
 * `docs/product-vision.md` が求めている。切り替えは要件に無いので、
 * **判断記録が要る**（この形は `docs/design/canvas` の付箋に残っている）。
 *
 * ここは読むだけ。 値をつまんで動かすのは原音設定エディタ（`PROFILE-M6`）で、
 * その描画面の性質は `Q-PLT-004` が閉じるまで決まらない。
 *
 * 音を選ぶ的をここに置かない。 選ぶのは波形の下の帯（`components/take-waveform`）
 * 1箇所だけ。**同じ札を2列並べていたので、押せるのは下の小さいほうだけ、
 * という状態になっていた。踏んだ。** ここは選ばれている音の名前を出すだけ。
 */
export const TakeValues = ({ otos, selected, durationMs, raw, onRaw }: TakeValuesProps) => {
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
          {(raw
            ? ([
                ["offset", `${oto.offset_ms.toFixed(3)} ms`],
                ["preutterance", `${oto.preutterance_ms.toFixed(3)} ms`],
                ["overlap", `${oto.overlap_ms.toFixed(3)} ms`],
                ["consonant", `${oto.consonant_ms.toFixed(3)} ms`],
                ["cutoff", `${oto.cutoff_ms.toFixed(3)} ms`],
              ] as const)
            : ([
                ["頭の余白", `${(oto.offset_ms / 1000).toFixed(3)} 秒`],
                ["歌い出しの位置", `${(oto.preutterance_ms / 1000).toFixed(3)} 秒`],
                ["前の音との重なり", `${(oto.overlap_ms / 1000).toFixed(3)} 秒`],
                ["伸ばさないところ", `${(oto.consonant_ms / 1000).toFixed(3)} 秒`],
                [
                  "終わりの余白",
                  `${((durationMs - usableSpan(oto, durationMs)[1]) / 1000).toFixed(3)} 秒`,
                ],
              ] as const)
          ).map(([label, value]) => (
            <div key={label} className="flex flex-col gap-2">
              <dt className="text-xs text-slate-11">{label}</dt>
              <dd className="m-0 select-text font-mono text-sm text-slate-12 tabular-nums">
                {value}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </>
  );
};
