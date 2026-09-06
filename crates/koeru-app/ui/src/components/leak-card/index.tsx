import { useMutation } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";

type LeakCardProps = {
  ready: boolean;
  /** 音高提示に使う音（MIDI）。 */
  midi: number;
  onStatus: (message: string) => void;
  onChecked: (leaking: boolean) => void;
};

/**
 * ガイドの回り込み検査（`TR-REC-24`）。
 *
 * 出力の種別だけでは足りない。 ドライバの自己申告で、
 * ヘッドホンと申告していても装着されている保証はない。
 * 回り込みは録音側でしか確認できない。
 *
 * これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。
 */
export const LeakCard = ({ ready, midi, onStatus, onChecked }: LeakCardProps) => {
  const run = useMutation({
    mutationFn: () => api.checkGuideLeak(midi),
    onMutate: () => onStatus("音を鳴らして、マイクに入るか確かめています"),
    onSuccess: (r) => {
      onChecked(r.leaking);
      onStatus(r.leaking ? "スピーカの音がマイクに入っています" : "マイクには入っていません");
    },
  });

  const play = useMutation({ mutationFn: () => api.playPitch(midi) });

  const result = run.data ?? null;
  const error = run.error ?? play.error;

  return (
    <Card title="音の回り込み">
      <div className="mt-3 flex flex-col gap-3">
        <p className="text-sm text-slate-11">
          音高を鳴らして、それがマイクに入らないか確かめます。
          <br />
          入ってしまうと、録った音すべてに混ざります。
        </p>

        <div className="flex items-center gap-3">
          <Button
            variant="secondary"
            onClick={() => run.mutate()}
            disabled={!ready || run.isPending}
          >
            {run.isPending ? "確かめています" : "確かめる"}
          </Button>
          {result !== null && !result.leaking && (
            <Button variant="ghost" onClick={() => play.mutate()}>
              音高を聞く
            </Button>
          )}
        </div>

        {result !== null &&
          (result.leaking ? (
            <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
              スピーカの音がマイクに入っています。
              イヤホンかヘッドホンを使うと、音高を聞きながら録れます。
              <br />
              このまま録ることもできますが、音高は鳴らしません。
            </p>
          ) : (
            <p className="rounded-lg bg-slate-3 px-4 py-3 text-sm text-slate-11">
              入っていません。音高を聞きながら録れます。
            </p>
          ))}

        {error !== null && (
          <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
            {errorMessage(error)}
          </p>
        )}
      </div>
    </Card>
  );
};
