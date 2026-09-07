import { useSuspenseQuery } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import type { SongView } from "~/lib/ipc";
import { songPlanQuery } from "~/lib/queries";

type SongDetailProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
  song: SongView;
  /** その行から録りはじめる。曲から収録へ直接入る（`TR-RCL-17`）。 */
  onRecordFrom: (rowId: string) => void;
};

/**
 * 選んだ曲の、あと録る行（`TR-RCL-16`、`TR-RCL-17`）。
 *
 * 行はフルリストの部分集合。 詰め直さないので、ここで録った分は
 * そのままフル方式の被覆になる。
 *
 * **曲から、その行の収録へ直接入る**（`DEC-PLT-024` の横移動）。
 * コレクションを経由せず、別のオブジェクトの詳細へ入る経路。
 */
export const SongDetail = ({ voiceId, song, onRecordFrom }: SongDetailProps) => {
  const { data: plan } = useSuspenseQuery(songPlanQuery(voiceId, song.id));
  const first = plan.rows[0];

  return (
    <Card title={song.title}>
      {plan.rows.length === 0 ? (
        <p className="text-sm text-slate-12">この曲に要る音は、すべて録れています。</p>
      ) : (
        <>
          {/*
            単位を1つの節に混ぜない。 録るのは行、そこから取れるのが音。

            「あと」ではなく「この」。 すぐ下に行が並んでいるので、
            一覧の側と同じ文にすると、同じ面に同じ文が2回出る。
          */}
          <p className="text-sm text-slate-12">
            この <span className="font-mono tabular-nums">{plan.rows.length}</span> 行（
            <span className="font-mono tabular-nums">{plan.covers}</span> 音）を録ると歌えます。
          </p>
          <p className="text-xs text-slate-11">
            読むのに{" "}
            <span className="font-mono tabular-nums">
              約 {Math.max(1, Math.round(plan.seconds / 60))}
            </span>{" "}
            分。
          </p>

          <ul className="flex flex-col gap-2">
            {plan.rows.map((r) => (
              <li
                key={r.row_id}
                className="flex h-9 items-center justify-between gap-3 rounded-lg bg-slate-3 px-3"
              >
                <span className="select-text text-sm text-slate-12">{r.text}</span>
                <span className="font-mono text-xs text-slate-11 tabular-nums">{r.units} 音</span>
              </li>
            ))}
          </ul>

          {plan.unreachable > 0 && (
            <p className="text-sm text-slate-11">
              このうち <span className="font-mono tabular-nums">{plan.unreachable}</span>{" "}
              音は、いまの作り方の録音リストに入っていません。近い音で置き換えて歌います。
            </p>
          )}

          {/*
            どの行から始まるかを名指す。 「この行から録る」では、上の一覧の
            どれを指しているのか分からなかった——印の付いた行が無い。
          */}
          {first !== undefined && (
            <Button variant="primary" onClick={() => onRecordFrom(first.row_id)}>
              「{first.text}」から録る
            </Button>
          )}
        </>
      )}
    </Card>
  );
};
