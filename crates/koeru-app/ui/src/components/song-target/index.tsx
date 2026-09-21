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
      <SongRange
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
