import { useMutation, useSuspenseQuery } from "@tanstack/react-query";

import { Breath } from "~/components/breath";
import { Card } from "~/components/card";
import { LiveWaveform } from "~/components/live-waveform";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "~/components/select";
import { api, errorMessage, micModeLabel } from "~/lib/ipc";
import { PROBE_MS } from "~/lib/levels";
import { devicesQuery } from "~/lib/queries";

type DeviceCardProps = {
  /** 選ばれているマイク。未選択なら `undefined`。 */
  deviceId: string | undefined;
  onDeviceChange: (id: string) => void;
  /** ストリームが開いているか。選ばれているだけでは、まだ音は流れていない。 */
  armed: boolean;
  /** ストリームが開いた。開くのが要る操作を、ここで押せるようにする。 */
  onArmed: () => void;
  onStatus: (message: string) => void;
};

/**
 * 録るときの音（`TR-REC-03`、`TR-REC-11`、`TR-REC-41`）。
 *
 * 設定の面に置く（`DEC-PLT-024`）。 マイクは音源に固定され、ゲインも
 * 音源の期間中は固定される。設定である以上、置き場所は設定面。
 * 収録の面に残すのは「いま何で録っているか」の1行だけ。
 *
 * OS 側の加工は事実として出す（`TR-REC-11`）。 判定ではなく、
 * 何が起きていて、どこを触ればよいかを書く。
 *
 * 残量が足りないときは「その残量で何件録れるか」を出す（`TR-REC-41`）。
 * 「足りません」だけでは、何を削れば足りるのか分からない。
 */
export const DeviceCard = ({
  deviceId,
  armed,
  onDeviceChange,
  onArmed,
  onStatus,
}: DeviceCardProps) => {
  const { data: devices } = useSuspenseQuery(devicesQuery());

  /**
   * マイクを開く。
   *
   * `arm_device` → `probe_input`（400ms 待つ）→ `estimate_space` の3手で
   * 1つの操作。 押してから半秒以上かかるので、その間は選び直させない——
   * 途中で別のマイクを開くと、どちらの結果が後に着くか決まらない。
   *
   * 結果は `arm.data` が持つ。 別に state を置くと、次のマイクを開いている
   * 最中に前のマイクの結果が出たままになる。
   */
  const arm = useMutation({
    mutationFn: async (id: string) => {
      const mode = await api.armDevice(id);
      const peak = await api.probeInput(PROBE_MS);
      const space = await api.estimateSpace();
      return { mode, peak, space };
    },
    onMutate: () => onStatus("入力を確かめています"),
    // 届いているかを一度だけ言う。以後の値は波形とメーターが持つ（`TR-REC-43`）。
    onSuccess: ({ peak }) => {
      onArmed();
      onStatus(peak > 0.000_001 ? "音が届いています" : "音が届いていません");
    },
  });

  const micMode = arm.data?.mode ?? null;
  const space = arm.data?.space ?? null;

  return (
    <Card title="録るときの音">
      <div className="flex flex-col gap-2">
        {/*
          名前は見えている字が持つ。 `aria-label` を置くと可視テキストを
          上書きして、選んでいるマイクの名前が消える（`TR-PLT-29`）。
        */}
        <span id="device-label" className="text-xs text-slate-11">
          マイク
        </span>
        {/* `exactOptionalPropertyTypes` なので、未選択は `value` を渡さない。 */}
        <Select
          {...(deviceId === undefined ? {} : { value: deviceId })}
          onValueChange={(id) => {
            onDeviceChange(id);
            arm.mutate(id);
          }}
          disabled={arm.isPending}
        >
          <SelectTrigger aria-labelledby="device-label">
            <SelectValue placeholder="マイクを選ぶ" />
          </SelectTrigger>
          <SelectContent>
            {devices.map((d) => (
              <SelectItem key={d.id} value={d.id}>
                {d.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>

      {arm.isPending && (
        <p className="flex items-center gap-2 text-xs text-slate-11">
          <Breath size="sm" />
          入力を確かめています
        </p>
      )}

      {/*
        開いていなければ波形を出さない。 選ばれているだけの状態がある——
        起動し直した直後は、選択は戻っていてもストリームは閉じている。
        出すと、届いていない音を「静か」として描くことになる。
      */}
      {armed && !arm.isPending && <LiveWaveform />}

      <p className="text-xs text-slate-11">録っているあいだ、この設定は変わりません。</p>

      {micMode !== null && micMode !== "Standard" && (
        <p className="rounded-lg bg-slate-3 px-3 py-2 text-sm text-slate-12">
          OS 側の音声処理が入っています（{micModeLabel(micMode)}）。
          システム設定のマイクモードを「標準」にすると、録った音がそのまま残ります。
        </p>
      )}

      {space !== null && !space.sufficient && (
        <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
          保存先の残りでは、あと {space.remaining_rows} 行のうち {space.rows_that_fit}{" "}
          行までしか録れません。
        </p>
      )}

      {arm.error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(arm.error)}
        </p>
      )}
    </Card>
  );
};
