import { useEffect, useRef, useState } from "react";

import { CalibrationCard } from "~/components/calibration-card";
import { LeakCard } from "~/components/leak-card";
import { LiveWaveform } from "~/components/live-waveform";
import { Spinner } from "~/components/spinner";
import { Card } from "~/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "~/components/ui/select";
import { api, type DeviceView, type MicModeView, micModeLabel, type SpaceView } from "~/lib/ipc";

type InputSetupProps = {
  /** 選ばれている入力デバイス。未選択なら `undefined`。 */
  deviceId: string | undefined;
  onDeviceChange: (id: string) => void;
  /** 回り込みの確認に使う音高（MIDI）。 */
  guideMidi: number;
  onStatus: (message: string) => void;
  onError: (cause: unknown) => void;
  /**
   * 回り込みを確認した結果。
   *
   * 親へ返すのは、音高提示を鳴らしてよいかがこの外側で決まるため
   * （`TR-REC-24`）。この面だけで閉じない。
   */
  onLeakChecked: (leaking: boolean) => void;
};

/**
 * 入力の面。マイクが使える状態かどうかを、ここだけで見せる。
 *
 * 監視と設定を分けてある。 波形とレベルは録る前から動き続けるもの
 * （`TR-REC-43`、`TR-REC-19`）なので常に見せる。デバイスの選択・校正・
 * 回り込みの確認は一度済ませれば触らないので、済んだら畳む——
 * 録る前に3枚のカードを読ませない。
 */
export const InputSetup = ({
  deviceId,
  onDeviceChange,
  guideMidi,
  onStatus,
  onError,
  onLeakChecked,
}: InputSetupProps) => {
  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [micMode, setMicMode] = useState<MicModeView | null>(null);
  const [space, setSpace] = useState<SpaceView | null>(null);
  const [leaking, setLeaking] = useState<boolean | null>(null);
  /**
   * デバイスを開いている最中か。
   *
   * 選ぶと `arm_device` → `probe_input`（400ms 待つ）→ `estimate_space` と
   * 続くので、押してから半秒以上かかる。 その間に選び直させない——
   * 途中で別のデバイスを開くと、どちらの結果が後に着くか決まらない。
   */
  const [arming, setArming] = useState(false);

  /**
   * 設定を開いているか。
   *
   * 済んだら一度だけ畳む。以後は本人の開閉に従う——勝手に開き直さない。
   */
  const [open, setOpen] = useState(true);
  const settled = useRef(false);

  const ready = deviceId !== undefined;

  useEffect(() => {
    api.listDevices().then(setDevices).catch(onError);
  }, [onError]);

  const choose = (next: string) => {
    onDeviceChange(next);
    onStatus("入力を確かめています");
    setArming(true);
    api
      .armDevice(next)
      .then((mode) => {
        setMicMode(mode);
        return api.probeInput(400);
      })
      .then((peak) => {
        // 届いているかを一度だけ言う。以後の値は波形とメーターが持つ（`TR-REC-43`）。
        onStatus(peak > 0.000_001 ? "入力が届いています" : "入力が届いていません");
        return api.estimateSpace();
      })
      .then(setSpace)
      .catch(onError)
      // 失敗しても下ろす。下ろさないと二度と選べなくなる。
      .finally(() => setArming(false));
  };

  /** 畳んだときに、何が済んでいるかを1行で見せる。 */
  const summary = [
    ready ? (devices.find((d) => d.id === deviceId)?.name ?? "選択済み") : "デバイス未選択",
    micMode !== null && micMode !== "Standard" ? `OS 処理あり（${micModeLabel(micMode)}）` : null,
    leaking === null ? "回り込み未確認" : leaking ? "回り込みあり" : "回り込みなし",
  ]
    .filter((x) => x !== null)
    .join(" · ");

  return (
    <Card title="マイク">
      {ready ? (
        <LiveWaveform />
      ) : (
        <p className="mt-3 text-sm text-slate-11">下から入力デバイスを選んでください。</p>
      )}

      {/*
        残量が足りないときは「その残量で何件録れるか」を出す（`TR-REC-41`）。
        「足りません」だけでは、何を削れば足りるのか分からない。
      */}
      {space !== null && !space.sufficient && (
        <p role="alert" className="mt-3 rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
          保存先の残量では、残り {space.remaining_rows} 件のうち {space.rows_that_fit}{" "}
          件までしか録れません。
        </p>
      )}

      {micMode !== null && micMode !== "Standard" && (
        <p className="mt-3 rounded-lg bg-slate-3 px-4 py-3 text-sm text-slate-11">
          OS 側の音声処理が入っています（{micModeLabel(micMode)}）。
          システム設定のマイクモードを「標準」にすると、録った音がそのまま残ります。
        </p>
      )}

      <details
        open={open}
        onToggle={(e) => setOpen(e.currentTarget.open)}
        className="mt-4 border-slate-6 border-t pt-3"
      >
        <summary className="cursor-pointer text-sm text-slate-11">
          入力の設定
          <span className="ml-2 font-mono text-xs tabular-nums">{summary}</span>
        </summary>

        <div className="mt-3 flex flex-col gap-3">
          {/* `exactOptionalPropertyTypes` なので、未選択は `value` を渡さない。 */}
          <Select
            {...(deviceId === undefined ? {} : { value: deviceId })}
            onValueChange={choose}
            disabled={arming}
          >
            {/*
              `aria-label` を置くと可視テキストを上書きして、
              選んでいるデバイス名が名前から消える。見出しを名前にする。
            */}
            <span id="device-label" className="text-sm text-slate-11">
              入力デバイス
            </span>
            <SelectTrigger aria-labelledby="device-label">
              <SelectValue placeholder="入力デバイスを選ぶ" />
            </SelectTrigger>
            {/* 待っていることを目でも出す。文言は読み上げ領域が持つ。 */}
            {arming ? (
              <span className="flex items-center gap-2 text-sm text-slate-11">
                <Spinner />
                入力を確かめています
              </span>
            ) : null}
            <SelectContent>
              {devices.map((d) => (
                <SelectItem key={d.id} value={d.id}>
                  {d.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          <CalibrationCard ready={ready} onStatus={onStatus} />

          <LeakCard
            ready={ready}
            midi={guideMidi}
            onStatus={onStatus}
            onChecked={(v) => {
              setLeaking(v);
              onLeakChecked(v);
              // 準備が済んだ出来事の側で畳む。effect で state を追いかけない。
              // 一度だけ。以後は本人の開閉に従う。
              if (!settled.current && ready) {
                settled.current = true;
                setOpen(false);
              }
            }}
          />
        </div>
      </details>
    </Card>
  );
};
