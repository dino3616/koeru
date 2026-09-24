import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";

import { Card } from "~/components/card";
import { SongRange } from "~/components/song-range";
import { api } from "~/lib/ipc";
import { ledgerKey, songNotesQuery } from "~/lib/queries";

type SongTargetProps = {
  voiceId: string;
  songId: string;
  title: string;
};

/**
 * 歌いたいところを目標にする（`TR-RCL-12`, `TR-RCL-16`）。
 *
 * 選んだ拍を歌えるようにする行を、その場で組み直して台帳へ足す
 * （`DEC-RCL-011`）。既にある行は消さない——録ったものが消える。
 */
export const SongTarget = ({ voiceId, songId, title }: SongTargetProps) => {
  const queryClient = useQueryClient();
  const { data: notes } = useSuspenseQuery(songNotesQuery(voiceId, songId));

  const repack = useMutation({
    mutationFn: (ranges: { from: number; to: number }[]) =>
      api.repackForSelection([{ song_id: songId, ranges }]),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  return (
    <Card title={`${title} のどこを歌うか`}>
      {/*
        曲が変わったら選び直しから始める。 **鍵を付けていなかった。**
        React は同じ位置の部品を使い回すので、選んだ添字が次の曲へ持ち越され、
        押すと別の曲の無関係なところを目標にしていた。

        effect で消さない（`react-conventions`）。 鍵を変えれば、
        持ち越しうる状態そのものが無くなる。
      */}
      <SongRange
        key={songId}
        notes={notes}
        building={repack.isPending}
        onRepack={(ranges) => repack.mutate(ranges)}
      />
      {repack.data !== undefined && (
        <p role="status" className="text-xs text-slate-11">
          <span className="font-mono tabular-nums">{repack.data}</span> 行を足しました。
        </p>
      )}
    </Card>
  );
};
