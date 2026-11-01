import { type CSSProperties, useId } from "react";

import { Icon, type IconName } from "~/components/icon";
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
 * カバレッジ・区切り・収録順・BPM・入力デバイス。 寸法は S04 のボードのまま。
 *
 * 字の大きさと色はツールバーの帯（13px の `text-2`）から受ける。 ここで決めるのは
 * 値の字（`text-1`）と、札の中の 12px だけ。
 *
 * 録る順は常に見えるところに出す（`TR-SYN-19`）。 「次に何を録るか」が
 * 理由もなく変わったように見えないため。
 *
 * テンポは刻みの ± だけ。 打ち込ませない——範囲と刻みの外の値を作らせない
 * （`DEC-REC-012`）。 数は Rust から受け、画面で決めない。
 *
 * 並べすぎると、ツールバーの左のパンくずが先に縮む（`ShellToolbarEnd`）。
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
  const orderId = useId();
  const tempoId = useId();
  const percent = coverage.total === 0 ? 0 : Math.round((coverage.done / coverage.total) * 100);
  const lower = tempo.bpm - tempo.stepBpm;
  const higher = tempo.bpm + tempo.stepBpm;
  const still = locked || pending;

  return (
    <div className="flex items-center gap-2.5 whitespace-nowrap">
      <p className="flex items-center gap-2">
        カバレッジ
        <span className="font-mono text-1 tabular-nums">{percent}%</span>
        {/* 値は字が持つ。 帯は見た目だけ。 */}
        <span aria-hidden="true" className="h-1 w-18 overflow-hidden rounded-full bg-surface-3">
          <span
            className="block h-full w-(--filled) bg-voice"
            // 埋まり具合は数で変わる。 変わる値は変数で渡し、当てるのはクラス。
            style={{ "--filled": `${percent}%` } as CSSProperties}
          />
        </span>
      </p>

      <span aria-hidden="true" className="mx-1.5 h-4 w-px bg-line" />

      <div className="flex items-center gap-4">
        <span className="flex items-center gap-2">
          <span id={orderId}>収録順</span>
          <Select value={mode} onValueChange={onModeChange} disabled={still}>
            <SelectTrigger size="sm" aria-labelledby={orderId}>
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
        </span>

        <span className="flex items-center gap-2">
          <span id={tempoId}>BPM</span>
          <span
            role="group"
            aria-labelledby={tempoId}
            className="flex h-7 items-center rounded-md border border-line bg-surface-1"
          >
            <Step
              icon="minus"
              label="BPM を下げる"
              disabled={still || lower < tempo.minBpm}
              onClick={() => onTempoChange(lower)}
            />
            <span className="w-9 border-x border-line text-center font-mono text-xs leading-6.5 text-1 tabular-nums">
              {tempo.bpm}
            </span>
            <Step
              icon="plus"
              label="BPM を上げる"
              disabled={still || higher > tempo.maxBpm}
              onClick={() => onTempoChange(higher)}
            />
          </span>
        </span>

        <Select
          value={deviceId ?? ""}
          onValueChange={onDeviceChange}
          disabled={still || devices.length === 0}
        >
          <SelectTrigger size="sm" aria-label="入力デバイス">
            <span aria-hidden="true" className="flex text-2">
              <Icon name="mic" size={13} />
            </span>
            <SelectValue
              placeholder={devices.length === 0 ? "マイクがありません" : "マイクを選ぶ"}
            />
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
    </div>
  );
};

/**
 * テンポの1刻み。 群の枠の内側に収まる 24×26 の的（S04）。
 *
 * `Button` を使わない。 群が枠と面を持ち、刻みは枠の中の区画として描くので、
 * 角丸も縁も要らない。 `TR-PLT-31` の 24px は満たす。
 */
const Step = ({
  icon,
  label,
  disabled,
  onClick,
}: {
  icon: IconName;
  label: string;
  disabled: boolean;
  onClick: () => void;
}) => (
  <button
    type="button"
    aria-label={label}
    disabled={disabled}
    onClick={onClick}
    className="flex h-6.5 w-6 items-center justify-center text-2 enabled:hover:text-1 disabled:opacity-45"
  >
    <Icon name={icon} size={14} />
  </button>
);
