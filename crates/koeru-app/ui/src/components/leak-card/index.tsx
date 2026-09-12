import { useMutation } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";

type LeakCardProps = {
  ready: boolean;
  /** 確かめるのに鳴らす音（MIDI）。 */
  midi: number;
  onStatus: (message: string) => void;
  onChecked: (leaking: boolean) => void;
};

/**
 * 音の回り込み（`TR-REC-24`）。
 *
 * 出力の種別だけでは足りない。 ドライバの自己申告で、
 * ヘッドホンと申告していても装着されている保証はない。
 * 回り込みは録音側でしか確かめられない。
 *
 * これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。
 *
 * 止めない。 回り込んでいても録れる。鳴らさないだけ。
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
      <p className="text-sm text-slate-12">
        音の高さを鳴らして、それがマイクに入らないか確かめます。
        入ってしまうと、録った音すべてに混ざります。
      </p>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => run.mutate()}
          disabled={!ready || run.isPending}
        >
          {run.isPending ? "確かめています" : "確かめる"}
        </Button>
        {result !== null && !result.leaking && (
          <Button variant="ghost" size="sm" onClick={() => play.mutate()}>
            音の高さを聞く
          </Button>
        )}
      </div>

      {result !== null &&
        (result.leaking ? (
          <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
            スピーカの音がマイクに入っています。イヤホンかヘッドホンを使うと、
            音の高さを聞きながら録れます。このままでも録れますが、音の高さは鳴らしません。
          </p>
        ) : (
          <p className="rounded-lg bg-slate-3 px-3 py-2 text-sm text-slate-12">
            入っていません。音の高さを聞きながら録れます。
          </p>
        ))}

      {error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(error)}
        </p>
      )}
    </Card>
  );
};
