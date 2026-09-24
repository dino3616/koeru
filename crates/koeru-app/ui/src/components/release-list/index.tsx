import { useMutation, useSuspenseQuery } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { api, errorMessage } from "~/lib/ipc";
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
 * 場所は出さない（`TR-PKG-45`）。 名前だけを出し、置き場所は OS の
 * ファイルマネージャに見せてもらう——**フォルダ操作を要求しないが、
 * 到達経路は残す。** 作れるのに手が届かないと、配り物として成立しない。
 *
 * 枠を持たない。 置く側の領域が枠になる（`docs/reports/design/direction.md`）。
 */
export const ReleaseList = ({ voiceId }: ReleaseListProps) => {
  const { data: releases } = useSuspenseQuery(releasesQuery(voiceId));
  const reveal = useMutation({ mutationFn: (seq: number) => api.revealRelease(seq) });

  if (releases.length === 0) {
    return <p className="text-sm text-slate-11">まだ作っていません。</p>;
  }

  return (
    <>
      <ul className="flex flex-col gap-3">
        {releases.map((r) => (
          <li key={r.seq} className="flex flex-col gap-1">
            <span className="select-text font-mono text-xs text-slate-12">{r.archive_name}</span>
            <span className="text-xs text-slate-11">
              {/*
                本人が付けた札を出す（`TR-PKG-44`）。 ファイル名は ASCII に
                落とすので、`正式版` のような札は名前の側から消える
                ——**そうすると、日本語の札を付けた回は連番でしか見分けられない。**
              */}
              {r.version === "" ? "呼び名なし" : r.version} · {shortDate(r.released_at)} ·{" "}
              <span className="font-mono tabular-nums">{r.alias_count}</span> 音
            </span>
            <Button
              variant="ghost"
              size="sm"
              className="self-start"
              onClick={() => reveal.mutate(r.seq)}
              aria-label={`${r.archive_name} の置き場所を開く`}
            >
              置き場所を開く
            </Button>
          </li>
        ))}
      </ul>
      {reveal.error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(reveal.error)}
        </p>
      )}
    </>
  );
};

/**
 * 日付だけを、手元の時間帯で出す。
 *
 * Rust が返すのは UTC の RFC 3339。 **先頭 10 文字を切ると UTC の日付が
 * 出る**ので、日本時間の朝に作ったものが前日として並ぶ。
 *
 * 時刻まで出さない。 1日に何度も作った人の一覧が数字で埋まる——
 * 見分けたいのは「いつ作ったか」なので、日で足りる。
 */
const shortDate = (rfc3339: string): string => {
  const at = new Date(rfc3339);
  if (Number.isNaN(at.getTime())) {
    // 読めない値でも一覧を壊さない。書いたものをそのまま出す。
    return rfc3339;
  }
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}`;
};
