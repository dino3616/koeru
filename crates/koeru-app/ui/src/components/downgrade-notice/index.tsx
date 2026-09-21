import { methodLabel } from "~/lib/labels";

type Downgrade = {
  method: string;
  bytes: number;
  sessions: number;
  span_days: number;
};

type DowngradeNoticeProps = {
  downgrades: readonly Downgrade[];
};

/** バイトを「約 N MB」にする。単位を省かない。 */
const megabytes = (bytes: number) => `約 ${Math.max(1, Math.round(bytes / 1024 / 1024))} MB`;

/**
 * 下位方式への書き出し（`TR-PKG-24`, `TR-PKG-25`）。
 *
 * **容量を書き出し前に出す。** 「oto.ini 1ファイル分」ではない——
 * 独立した音源ルート・独立した ZIP になるので、WAV が複製されて
 * 元とほぼ同等の容量がもう1本できる。
 *
 * 素材の由来も出す（`DEC-RCL-007`）。 跨ぐ収録セッションの数と、
 * 最初から最後までの間隔。**「声質が揃っている」とは言わない**——
 * 検知する手段が無いので、事実だけを置く。
 *
 * 同じ ZIP には入れない（`TR-PKG-25`）。 接頭辞を付ければ単独音として
 * 使えなくなり、付けなければエイリアスが衝突する。両立しない。
 */
export const DowngradeNotice = ({ downgrades }: DowngradeNoticeProps) => {
  if (downgrades.length === 0) return null;

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-slate-7 bg-slate-3 p-3">
      <p className="text-sm font-semibold text-slate-12">別の作り方でも出せます</p>
      <ul className="flex flex-col gap-3">
        {downgrades.map((d) => (
          <li key={d.method} className="flex flex-col gap-1">
            <span className="text-sm text-slate-12">{methodLabel(d.method)}</span>
            <dl className="flex flex-col gap-1">
              <div className="flex items-baseline justify-between gap-3">
                <dt className="text-xs text-slate-11">増える容量</dt>
                <dd className="font-mono text-sm text-slate-12 tabular-nums">
                  {megabytes(d.bytes)}
                </dd>
              </div>
              <div className="flex items-baseline justify-between gap-3">
                <dt className="text-xs text-slate-11">録った回</dt>
                <dd className="font-mono text-sm text-slate-12 tabular-nums">
                  {d.sessions} 回 · {d.span_days} 日のあいだ
                </dd>
              </div>
            </dl>
          </li>
        ))}
      </ul>
      <p className="text-xs text-slate-11">
        別のファイルとして作ります。音は同じものを複製するので、そのぶん容量が増えます。
      </p>
      <p className="text-xs text-slate-11">
        何回かに分けて録った音が混ざります。声の揃い具合は確かめていません。
      </p>
    </div>
  );
};
