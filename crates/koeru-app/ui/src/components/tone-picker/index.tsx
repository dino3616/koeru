import { useSuspenseQueries } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "~/components/button";
import { toneOptionsQuery, toneSuggestionsQuery } from "~/lib/queries";

type TonePickerProps = {
  /** いま選んでいる収録音高（MIDI）。昇順。 */
  tones: readonly number[];
  onChange: (tones: number[]) => void;
};

/**
 * 収録音高を選ぶ（`TR-RCL-01`, `TR-RCL-06`）。
 *
 * **本数も音高も本人が決める。** 作り方（単独音 / 連続音 / CVVC）とは別の選択で、
 * 「多音階連続音」のような組で固定しない——固定すると、2本にすることも、
 * C3 と A4 だけにすることもできない。
 *
 * 間隔で咎めない。 以前は 5〜9 半音を外れたら警告していたが、その数字に
 * 出どころが無かった（`TR-RCL-06`）。推奨は出すが、選択は止めない。
 *
 * 録る量は本数に比例する。 リストは本数を変えても同じものを録る
 * （`TR-RCL-26`）ので、増やすぶんだけ時間が伸びる。そう1行で書く。
 *
 * MIDI 番号を画面に出さない（`TR-REC-25`）。 出すのは英語音名。
 */
export const TonePicker = ({ tones, onChange }: TonePickerProps) => {
  const addId = useId();
  const [{ data: options }, { data: suggestions }] = useSuspenseQueries({
    queries: [toneOptionsQuery(), toneSuggestionsQuery()],
  });
  const [pending, setPending] = useState<string>("");

  const nameOf = (midi: number) => options.find((o) => o.midi === midi)?.name ?? "";
  const sorted = (next: number[]) => [...new Set(next)].sort((a, b) => a - b);

  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-wrap gap-2">
        {tones.map((midi) => (
          <li key={midi}>
            <span className="flex h-9 items-center gap-1 rounded-lg border border-slate-7 bg-slate-3 pl-3 text-sm text-slate-12">
              <span className="font-mono tabular-nums">{nameOf(midi)}</span>
              {/*
                最後の1本は外させない。 0 本のプロジェクトは作れない
                （Rust も `tone.empty` で断る）ので、押せる的として出さない。
              */}
              {tones.length > 1 && (
                <Button
                  size="sm"
                  variant="ghost"
                  aria-label={`${nameOf(midi)} を外す`}
                  onClick={() => onChange(tones.filter((t) => t !== midi))}
                >
                  外す
                </Button>
              )}
            </span>
          </li>
        ))}
      </ul>

      <div className="flex flex-wrap items-end gap-2">
        <span className="flex flex-col gap-2">
          <label className="text-xs text-slate-11" htmlFor={addId}>
            音の高さを足す
          </label>
          <select
            id={addId}
            value={pending}
            onChange={(e) => setPending(e.target.value)}
            className="h-11 select-text rounded-lg border border-slate-7 bg-slate-3 px-3 text-sm text-slate-12"
          >
            <option value="">選んでください</option>
            {options
              .filter((o) => !tones.includes(o.midi))
              .map((o) => (
                <option key={o.midi} value={String(o.midi)}>
                  {o.name}
                </option>
              ))}
          </select>
        </span>
        <Button
          variant="secondary"
          disabled={pending === ""}
          onClick={() => {
            onChange(sorted([...tones, Number(pending)]));
            setPending("");
          }}
        >
          足す
        </Button>
      </div>

      {/*
        推奨は出すが、選択は止めない（`TR-RCL-06`）。
        出発点を置かないと、84 個の選択肢の前で手が止まる。
      */}
      <div className="flex flex-col gap-2">
        <span className="text-xs text-slate-11">迷ったら</span>
        <ul className="flex flex-wrap gap-2">
          {suggestions.map((s) => (
            <li key={s.label}>
              <Button
                size="sm"
                variant="ghost"
                aria-label={`${s.label}（${s.midi.map(nameOf).join(" ")}）にする`}
                onClick={() => onChange(sorted([...s.midi]))}
              >
                {s.label}
              </Button>
            </li>
          ))}
        </ul>
      </div>

      <p className="text-xs text-slate-11">
        同じ行を、選んだ高さのぶんだけ録ります。{tones.length} 本なら {tones.length} 周です。
      </p>
    </div>
  );
};
