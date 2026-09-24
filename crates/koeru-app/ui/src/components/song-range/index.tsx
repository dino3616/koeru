import { useState } from "react";

import { Button } from "~/components/button";
import type { NoteView } from "~/lib/ipc";
import { cx } from "~/lib/tv";

type SongRangeProps = {
  /** 曲のノート列（`TR-RCL-12`）。 */
  notes: readonly NoteView[];
  /** 選んだ範囲で録音リストを詰め直す（`TR-RCL-16`）。 */
  onRepack: (ranges: { from: number; to: number }[]) => void;
  /** 詰め直している最中か。 */
  building: boolean;
};

/**
 * 歌いたいところを選ぶ（`TR-RCL-12`, `TR-RCL-16`）。
 *
 * > ファイル全体だけでなく、任意のノート群を選んで目標にできる（サビだけ、など）
 *
 * 選んだところを歌えるようにする行を、その場で組み直す（`DEC-RCL-011`）。
 * **フルリストから行を選ぶのではない。** 行の途中でやめられないので、
 * 部分集合だと読む量が跳ね上がる。
 *
 * 拍を1つずつ押して選ぶ。 範囲の指定を数字で打たせない——どの拍がどの歌詞かが
 * 見えていないと「サビだけ」を指せない。
 */
export const SongRange = ({ notes, onRepack, building }: SongRangeProps) => {
  const [picked, setPicked] = useState<ReadonlySet<number>>(new Set());

  const toggle = (i: number) => {
    const next = new Set(picked);
    if (next.has(i)) next.delete(i);
    else next.add(i);
    setPicked(next);
  };

  /** 連続した添字を `[from, to)` へ畳む。飛びは別の範囲にする。 */
  const ranges = () => {
    const sorted = [...picked].sort((a, b) => a - b);
    const out: { from: number; to: number }[] = [];
    for (const i of sorted) {
      const last = out.at(-1);
      if (last !== undefined && last.to === i) last.to = i + 1;
      else out.push({ from: i, to: i + 1 });
    }
    return out;
  };

  return (
    <div className="flex flex-col gap-3">
      <p className="text-xs text-slate-11">
        歌いたいところを選びます。選ばなければ曲ぜんぶが目標になります。
      </p>
      <ul className="flex flex-wrap gap-1">
        {notes.map((n, i) => (
          // 同じ歌詞が何度も出る。位置でしか区別できない。
          <li key={`${n.lyric}-${i}`}>
            <button
              type="button"
              aria-pressed={picked.has(i)}
              onClick={() => toggle(i)}
              className={cx(
                "flex min-w-11 flex-col items-center rounded-lg border px-2 py-1",
                picked.has(i)
                  ? "border-slate-9 bg-slate-5 text-slate-12"
                  : "border-slate-7 bg-slate-3 text-slate-11 hover:bg-slate-4",
              )}
            >
              <span className="text-base">{n.lyric}</span>
              <span className="font-mono text-xs tabular-nums">{n.tone}</span>
            </button>
          </li>
        ))}
      </ul>
      <div className="flex items-center gap-3">
        <Button variant="primary" onClick={() => onRepack(ranges())} disabled={building}>
          {building ? "作っています" : "ここを歌えるようにする"}
        </Button>
        <span className="font-mono text-xs text-slate-11 tabular-nums">
          {picked.size === 0 ? "曲ぜんぶ" : `${picked.size} 拍`}
        </span>
      </div>
    </div>
  );
};
