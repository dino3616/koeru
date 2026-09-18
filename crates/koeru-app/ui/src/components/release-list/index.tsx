import { useSuspenseQuery } from "@tanstack/react-query";

import { releasesQuery } from "~/lib/queries";

type ReleaseListProps = {
  voiceId: string;
};

/**
 * これまでに作った配り物（`TR-PKG-44`）。
 *
 * 記録は消えない。 一度書いたものは変わらない（スキーマのトリガが止める）。
 * 同じ呼び名で2回作っても、連番が違うので別のものとして残る。
 *
 * 場所は出さない（`TR-PKG-45`）。 名前だけを出す。
 *
 * 枠を持たない。 置く側の領域が枠になる（`docs/design/direction.md`）。
 */
export const ReleaseList = ({ voiceId }: ReleaseListProps) => {
  const { data: releases } = useSuspenseQuery(releasesQuery(voiceId));

  if (releases.length === 0) {
    return <p className="text-sm text-slate-11">まだ作っていません。</p>;
  }

  return (
    <ul className="flex flex-col gap-2">
      {releases.map((r) => (
        <li key={r.seq} className="flex flex-col gap-1">
          <span className="select-text font-mono text-xs text-slate-12">{r.archive_name}</span>
          <span className="text-xs text-slate-11">
            {shortDate(r.released_at)} ·{" "}
            <span className="font-mono tabular-nums">{r.alias_count}</span> 音
          </span>
        </li>
      ))}
    </ul>
  );
};

/**
 * 日付だけを出す。
 *
 * Rust が返すのは RFC 3339。 時刻まで出すと、1日に何度も作った人の一覧が
 * 数字で埋まる——見分けたいのは「いつ作ったか」なので、日で足りる。
 */
const shortDate = (rfc3339: string): string => rfc3339.slice(0, 10);
