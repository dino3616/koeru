import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";
import type { SongView } from "~/lib/ipc";
import { ledgerKey } from "~/lib/queries";

type SongKeyProps = {
  song: SongView;
};

/** 半音を「+3 半音」「そのまま」にする。符号を省かない。 */
const label = (semitones: number) =>
  semitones === 0 ? "そのまま" : `${semitones > 0 ? "+" : ""}${semitones} 半音`;

/** 採れるキー。1オクターブの上下まで——それを超えると元の曲と別の曲になる。 */
const CHOICES = Array.from({ length: 25 }, (_, i) => i - 12);

/**
 * 曲のキー（`TR-SYN-15`, `DEC-SYN-012`）。
 *
 * **KOERU は勝手に移調しない。** 以前は収録音高に近づくよう曲全体を裏で
 * 動かし、その結果を「いま歌えます」と出していた。本人が「この曲でこの声は
 * どう聴こえるか」を確かめるために入れた曲を黙って別の調にすると、
 * 確かめた結果が別の曲のものになる。
 *
 * 届かないときは、事実と道を出す。 何半音ぶん遠いか、どのキーなら届くか、
 * どの収録音高を足せば**このキーのまま**届くか。選ぶのは本人。
 *
 * **押せなくしない。** 基準の ±7 半音・二乗平均 4 半音に実測の裏付けが無い
 * （`reclist.toml` の領域リスク）。根拠の無い線で鳴らすことを禁じない。
 */
export const SongKey = ({ song }: SongKeyProps) => {
  const selectId = useId();
  const queryClient = useQueryClient();

  const move = useMutation({
    mutationFn: (semitones: number) => api.setSongTranspose(song.id, semitones),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  const suggested = song.recommended_transpose;

  return (
    <Card title={`${song.title} のキー`}>
      <div className="flex flex-wrap items-end gap-2">
        <span className="flex flex-col gap-2">
          <label className="text-xs text-slate-11" htmlFor={selectId}>
            この曲を動かす
          </label>
          <select
            id={selectId}
            value={String(song.transpose)}
            disabled={move.isPending}
            onChange={(e) => move.mutate(Number(e.target.value))}
            className="h-11 select-text rounded-lg border border-slate-7 bg-slate-3 px-3 text-sm text-slate-12"
          >
            {CHOICES.map((c) => (
              <option key={c} value={String(c)}>
                {label(c)}
              </option>
            ))}
          </select>
        </span>
      </div>

      {song.previewable ? (
        <p className="text-sm text-slate-12">このキーなら、収録した高さの近くで鳴らせます。</p>
      ) : (
        <>
          {/*
            何が起きるかを書く。 良し悪しは言わない（`TR-SYN-20`）。
            「歌えない」ではなく「音を引き伸ばすので声が変わる」。
          */}
          <p className="text-sm text-slate-12">
            このキーだと、録った高さから遠い音があります。引き伸ばすぶん、声が変わって聞こえます。
          </p>
          <ul className="flex flex-col gap-2">
            {suggested !== song.transpose && (
              <li className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs text-slate-11">
                  {label(suggested)}にすると、録った高さの近くに収まります
                </span>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={move.isPending}
                  onClick={() => move.mutate(suggested)}
                >
                  そのキーにする
                </Button>
              </li>
            )}
            {song.rescuing_tone !== null && (
              // 収録音高は作成時に確定する（`TR-REC-25`）ので、いまからは足せない。
              // 足せるように書かない——押せない道を示すことになる。
              <li className="text-xs text-slate-11">
                キーを変えたくないなら、
                <span className="font-mono tabular-nums"> {song.rescuing_tone} </span>
                でも録った音源なら、このキーのまま届きます。次に作るときの目安に。
              </li>
            )}
          </ul>
        </>
      )}

      <p className="text-xs text-slate-11">
        曲そのものは書き換えません。鳴らすときに、この分だけ動かします。
      </p>

      {move.error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(move.error)}
        </p>
      )}
    </Card>
  );
};
