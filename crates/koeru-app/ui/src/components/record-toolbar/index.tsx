import type { CSSProperties } from "react";

import { Button } from "~/components/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "~/components/select";
import type { DeviceView, GuideTempo } from "~/lib/ipc";
import { RECORDING_ORDERS, recordingOrderLabel } from "~/lib/labels";

type RecordToolbarProps = {
  /** 録った数と、録る数。 枠を選んでいれば枠の行、選んでいなければ版全体の単位。 */
  coverage: { done: number; total: number };
  /** いまの録る順（`TR-SYN-19`）。 */
  mode: string;
  onModeChange: (mode: string) => void;
  tempo: GuideTempo;
  /** テンポを変えたとき。 範囲と刻みの中の値だけが来る（`DEC-REC-012`）。 */
  onTempoChange: (bpm: number) => void;
  devices: readonly DeviceView[];
  /** 選んでいるマイク。 まだ選んでいなければ `undefined`。 */
  deviceId: string | undefined;
  onDeviceChange: (deviceId: string) => void;
  /**
   * 録っているあいだ。 どれも変えさせない。
   *
   * テンポを変えると開いている収録の区間を切って始め直す（`TR-REC-30`）ので、
   * 録音の途中で変えるとテイクがどのテンポかを言えなくなる。 マイクは Rust が
   * `app.already_recording` で断るが、押せる的として出さない。
   */
  locked: boolean;
  /** 変えている最中。 返るまで同じものを押させない。 */
  pending?: boolean;
};

/**
 * 収録の画面のツールバーの右端（`ShellToolbarEnd` に入れる）。 左から
 * カバレッジ・録る順・テンポ・マイク。 骨格は S04。
 *
 * 録る順は常に見えるところに出す（`TR-SYN-19`）。 「次に何を録るか」が
 * 理由もなく変わったように見えないため。
 *
 * テンポは刻みの ± だけ。 打ち込ませない——範囲と刻みの外の値を作らせない
 * （`DEC-REC-012`）。 数は Rust から受け、画面で決めない。
 *
 * 並べすぎると、ツールバーの左のパンくずが先に縮む（`ShellToolbarEnd`）。
 * 字は最小にして、名前は読み上げのほうで言う。
 */
export const RecordToolbar = ({
  coverage,
  mode,
  onModeChange,
  tempo,
  onTempoChange,
  devices,
  deviceId,
  onDeviceChange,
  locked,
  pending = false,
}: RecordToolbarProps) => {
  const percent = coverage.total === 0 ? 0 : Math.round((coverage.done / coverage.total) * 100);
  const lower = tempo.bpm - tempo.stepBpm;
  const higher = tempo.bpm + tempo.stepBpm;
  const still = locked || pending;

  return (
    <div className="flex h-full items-center gap-3">
      <p className="flex items-center gap-2 text-xs text-2">
        カバレッジ
        <span className="font-mono text-md text-1 tabular-nums">{percent}%</span>
        {/* 値は字が持つ。 帯は見た目だけ。 */}
        <span aria-hidden="true" className="h-1.5 w-12 overflow-hidden rounded-full bg-surface-3">
          <span
            className="block h-full w-(--filled) rounded-full bg-voice"
            // 埋まり具合は数で変わる。 変わる値は変数で渡し、当てるのはクラス。
            style={{ "--filled": `${percent}%` } as CSSProperties}
          />
        </span>
      </p>

      <Select value={mode} onValueChange={onModeChange} disabled={still}>
        <SelectTrigger size="sm" aria-label="録る順">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {RECORDING_ORDERS.map((m) => (
            <SelectItem key={m} value={m}>
              {recordingOrderLabel(m)}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <div role="group" aria-label="ガイドのテンポ" className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="sm"
          aria-label="テンポを下げる"
          disabled={still || lower < tempo.minBpm}
          onClick={() => onTempoChange(lower)}
        >
          −
        </Button>
        <span className="min-w-14 text-center font-mono text-md text-1 tabular-nums">
          {tempo.bpm}
          <span className="ml-1 text-xs text-2">BPM</span>
        </span>
        <Button
          variant="ghost"
          size="sm"
          aria-label="テンポを上げる"
          disabled={still || higher > tempo.maxBpm}
          onClick={() => onTempoChange(higher)}
        >
          ＋
        </Button>
      </div>

      <Select
        value={deviceId ?? ""}
        onValueChange={onDeviceChange}
        disabled={still || devices.length === 0}
      >
        <SelectTrigger size="sm" aria-label="マイク">
          <SelectValue placeholder={devices.length === 0 ? "マイクがありません" : "マイクを選ぶ"} />
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
  );
};
