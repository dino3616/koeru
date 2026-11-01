import { type KeyboardEvent, type ReactNode, useRef } from "react";

import { SETTINGS_TABS, type SettingsTab } from "~/lib/settings-tabs";
import { tv } from "~/lib/tv";

const tab = tv({
  variants: {
    selected: {
      true: "bg-slate-4 font-semibold text-slate-12",
      false: "text-slate-11 hover:bg-slate-3 hover:text-slate-12",
    },
  },
});

const tabId = (id: SettingsTab) => `settings-tab-${id}`;
const panelId = (id: SettingsTab) => `settings-panel-${id}`;

type SettingsTabsProps = {
  selected: SettingsTab;
  onSelect: (tab: SettingsTab) => void;
  /** 選んだタブの中身。 */
  children: ReactNode;
};

/**
 * 設定の外枠。 左にタブの列、右に選んだタブの中身。
 *
 * 選んだ時点で中身が替わる（矢印で動けばそのまま選ぶ）。 移ってから Enter を
 * 求めると、キーボードだけの人だけが1手多い。
 *
 * 選びは持たない。 どれを選んでいるかは URL が正本（`tab`）で、ここは
 * 受け取って描くだけ。 持つと、URL と見えているものが食い違う。
 *
 * Tab で入れるのは選んでいるタブ1つだけ（roving tabindex）。 6つ全部が
 * Tab の停止点になると、設定を通り過ぎるだけの人が6回押す。
 * 向きは縦なので、上下が前後。 左右も同じに扱う——縦の列で左右を押す人もいる。
 *
 * 中身の面は Tab で入れる。 中身に操作できるものが無いタブでも、
 * キーボードだけの人が中身まで辿り着ける。
 */
export const SettingsTabs = ({ selected, onSelect, children }: SettingsTabsProps) => {
  const buttons = useRef(new Map<SettingsTab, HTMLButtonElement>());

  const move = (to: SettingsTab) => {
    onSelect(to);
    buttons.current.get(to)?.focus();
  };

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    const at = SETTINGS_TABS.findIndex((t) => t.id === selected);
    const last = SETTINGS_TABS.length - 1;
    const next = {
      ArrowDown: at === last ? 0 : at + 1,
      ArrowRight: at === last ? 0 : at + 1,
      ArrowUp: at === 0 ? last : at - 1,
      ArrowLeft: at === 0 ? last : at - 1,
      Home: 0,
      End: last,
    }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const to = SETTINGS_TABS[next]?.id;
    if (to !== undefined) move(to);
  };

  return (
    <div className="flex h-full min-h-0 gap-2">
      <div
        role="tablist"
        aria-orientation="vertical"
        aria-label="設定の項目"
        className="flex w-48 flex-shrink-0 flex-col gap-1 p-2"
      >
        {SETTINGS_TABS.map((t) => (
          <button
            key={t.id}
            ref={(el) => {
              if (el === null) buttons.current.delete(t.id);
              else buttons.current.set(t.id, el);
            }}
            type="button"
            role="tab"
            id={tabId(t.id)}
            aria-selected={t.id === selected}
            aria-controls={panelId(t.id)}
            tabIndex={t.id === selected ? 0 : -1}
            onClick={() => onSelect(t.id)}
            onKeyDown={onKeyDown}
            className={`flex h-11 items-center rounded-lg px-3 text-left text-sm ${tab({ selected: t.id === selected })}`}
          >
            {t.label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={panelId(selected)}
        aria-labelledby={tabId(selected)}
        tabIndex={0}
        className="min-w-0 flex-1 overflow-y-auto rounded-xl border border-slate-6 bg-slate-1 p-6"
      >
        {children}
      </div>
    </div>
  );
};
