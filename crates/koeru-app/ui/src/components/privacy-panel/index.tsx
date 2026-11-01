import { type ReactNode, useId } from "react";

import { Button } from "~/components/button";
import { Switch } from "~/components/switch";
import type { ConsentState, ConsentSubject, PrivacySettings } from "~/lib/ipc";

type PrivacyPanelProps = {
  settings: PrivacySettings;
  /** 本人が1つのスイッチを切り替えた。 もう片方は動かさない（`TR-PKG-57`）。 */
  onChoose: (subject: ConsentSubject, granted: boolean) => void;
  /** 保存できなかったとき、本人に見せる文。 無ければ `null`。 */
  problem: string | null;
};

/**
 * 送ってよいのは「送ってよい」と答えたときだけ。 訊いていない・撤回したは、
 * どちらも送らないので、スイッチの上ではどちらも切れている。
 */
const isGranted = (state: ConsentState) => state === "GRANTED";

/**
 * 最後に送った日時。 読めない日時は、読めないまま見せる——
 * 空にすると「送っていない」と区別がつかない。
 */
const sentAt = (iso: string | null): ReactNode => {
  if (iso === null) return "まだ送っていません";
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  return (
    <time dateTime={iso}>
      {new Intl.DateTimeFormat("ja-JP", { dateStyle: "medium", timeStyle: "short" }).format(at)}
    </time>
  );
};

/**
 * 設定のプライバシーのタブ（`TR-PKG-54`）。
 *
 * 利用状況とクラッシュ報告は別の同意の単位で、どちらも既定はオフ
 * （`TR-PKG-57`）。 1つずつ切り替える。
 *
 * 保存の失敗は、常設の `status` へ入れる。 失敗したときだけ挿し込むと、
 * 支援技術が変化として拾えず読まれない。
 *
 * 見出しは `h2`。 画面の `h1` は設定そのもので、その下に置かれる。
 */
export const PrivacyPanel = ({ settings, onChoose, problem }: PrivacyPanelProps) => {
  const sendId = useId();
  const recordId = useId();
  const reasonId = useId();
  return (
    <div className="flex flex-col gap-6">
      <div className="flex items-baseline gap-3">
        <h2 className="text-xl font-semibold text-slate-12">プライバシー</h2>
        <p className="text-xs text-slate-11">既定はどちらもオフ</p>
      </div>

      <p role="status" className="text-sm text-red-11 empty:hidden">
        {problem}
      </p>

      <section aria-labelledby={sendId} className="flex flex-col gap-2">
        <h3 id={sendId} className="text-xs text-slate-11">
          送るもの
        </h3>
        <ul className="flex flex-col divide-y divide-slate-7 rounded-xl border border-slate-7 bg-slate-2">
          <li className="px-4 py-3">
            <Switch
              label="利用状況"
              description="画面への到達・離脱した場所"
              checked={isGranted(settings.usage)}
              onCheckedChange={(granted) => onChoose("USAGE", granted)}
            />
          </li>
          <li className="px-4 py-3">
            <Switch
              label="クラッシュ報告"
              checked={isGranted(settings.crashReports)}
              onCheckedChange={(granted) => onChoose("CRASH_REPORTS", granted)}
            />
          </li>
        </ul>
      </section>

      <section aria-labelledby={recordId} className="flex flex-col gap-2">
        <h3 id={recordId} className="text-xs text-slate-11">
          記録
        </h3>
        <ul className="flex flex-col divide-y divide-slate-7 rounded-xl border border-slate-7 bg-slate-2">
          <li className="flex items-center justify-between gap-4 px-4 py-3">
            <span className="text-sm text-slate-12">最後に送った日時</span>
            <span className="text-sm text-slate-11">{sentAt(settings.lastSentAt)}</span>
          </li>
          <li className="flex items-center justify-between gap-4 px-4 py-3">
            <div className="flex min-w-0 flex-col gap-1">
              <span className="text-sm text-slate-12">送った内容</span>
              <span id={reasonId} className="text-xs text-slate-11">
                送った内容を読む画面はまだありません
              </span>
            </div>
            <Button disabled aria-describedby={reasonId}>
              記録を開く…
            </Button>
          </li>
        </ul>
      </section>
    </div>
  );
};
