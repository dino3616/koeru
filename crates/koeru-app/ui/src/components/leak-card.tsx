import { useState } from "react";

import { Button } from "~/components/ui/button";
import { Card, CardTitle } from "~/components/ui/card";
import { type LeakView, api, errorMessage } from "~/lib/ipc";

type LeakCardProps = {
  ready: boolean;
  /** 音高提示に使う音（MIDI）。 */
  midi: number;
  onStatus: (message: string) => void;
  onChecked: (leaking: boolean) => void;
};

/**
 * ガイドの回り込み検査（TR-REC-24）。
 *
 * **出力の種別だけでは足りない。** ドライバの自己申告で、
 * ヘッドホンと申告していても装着されている保証はない。
 * **回り込みは録音側でしか確認できない。**
 *
 * これを置かないと、**全テイクにガイドが混入した音源が完成に到達しうる。**
 */
export const LeakCard = ({ ready, midi, onStatus, onChecked }: LeakCardProps) => {
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<LeakView | null>(null);
  const [error, setError] = useState<string | null>(null);

  const run = () => {
    setRunning(true);
    setError(null);
    onStatus("音を鳴らして、マイクに入るか確かめています");
    api
      .checkGuideLeak(midi)
      .then((r) => {
        setResult(r);
        onChecked(r.leaking);
        onStatus(r.leaking ? "スピーカの音がマイクに入っています" : "マイクには入っていません");
      })
      .catch((e: unknown) => setError(errorMessage(e)))
      .finally(() => setRunning(false));
  };

  return (
    <Card>
      <CardTitle>音の回り込み</CardTitle>

      <div className="mt-3 flex flex-col gap-3">
        <p className="text-sm text-text-dim">
          音高を鳴らして、それがマイクに入らないか確かめます。
          <br />
          入ってしまうと、録った音すべてに混ざります。
        </p>

        <div className="flex items-center gap-3">
          <Button variant="secondary" onClick={run} disabled={!ready || running}>
            {running ? "確かめています" : "確かめる"}
          </Button>
          {result !== null && !result.leaking && (
            <Button
              variant="ghost"
              onClick={() => {
                api.playPitch(midi).catch((e: unknown) => setError(errorMessage(e)));
              }}
            >
              音高を聞く
            </Button>
          )}
        </div>

        {result !== null &&
          (result.leaking ? (
            <p
              role="alert"
              className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text"
            >
              スピーカの音がマイクに入っています。
              イヤホンかヘッドホンを使うと、音高を聞きながら録れます。
              <br />
              このまま録ることもできますが、音高は鳴らしません。
            </p>
          ) : (
            <p className="rounded-lg bg-surface-2 px-4 py-3 text-sm text-text-dim">
              入っていません。音高を聞きながら録れます。
            </p>
          ))}

        {error !== null && (
          <p
            role="alert"
            className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text"
          >
            {error}
          </p>
        )}
      </div>
    </Card>
  );
};
