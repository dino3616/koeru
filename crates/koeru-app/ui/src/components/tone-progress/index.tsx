import { Card } from "~/components/card";

type ToneProgress = {
  /** 英語音名（`TR-REC-25`）。 */
  tone: string;
  done: number;
  total: number;
};

type ToneProgressProps = {
  byTone: readonly ToneProgress[];
};

/**
 * 音高ごとの消化率（`TR-RCL-26`）。
 *
 * > 進捗表示では「いま歌える曲の数」を音高を跨いだ実際の判定結果で出し、
 * > 音高ごとの消化率は詳細表示に置く
 *
 * **足し合わせた1本の帯にしない。** 3音高のうち1本だけ録り終えても、
 * 音域の広い曲は歌えない。「33%」と出すと、あと3分の2でできると読める。
 *
 * 単音階では出さない。 1本しかないところに内訳は無く、
 * 全体の進みと同じものを2度置くことになる。
 */
export const ToneProgressList = ({ byTone }: ToneProgressProps) => {
  if (byTone.length <= 1) return null;

  return (
    <Card title="音の高さごと">
      <ul className="flex flex-col gap-3">
        {byTone.map((t) => (
          <li key={t.tone} className="flex flex-col gap-1">
            <div className="flex items-baseline justify-between gap-3">
              <span className="font-mono text-sm text-slate-12">{t.tone}</span>
              {/* 色だけで伝えない。数と語を並べる。 */}
              <span className="font-mono text-sm text-slate-11 tabular-nums">
                {t.done} / {t.total} 行
              </span>
            </div>
            <meter
              className="h-2 w-full"
              min={0}
              max={t.total}
              value={t.done}
              aria-label={`${t.tone} の進み`}
            >
              {t.done} / {t.total} 行
            </meter>
          </li>
        ))}
      </ul>
      <p className="text-xs text-slate-11">
        高さごとに別で数えます。1つ録り終えても、音域の広い曲はまだ歌えません。
      </p>
    </Card>
  );
};
