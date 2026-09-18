import { useSuspenseQuery } from "@tanstack/react-query";

import { packageContentsQuery } from "~/lib/queries";

type PackageContentsProps = {
  voiceId: string;
};

/** 音源ルート直下に出るもの。ここから外は素材のフォルダ。 */
const ROOT_FILES = 8;

/**
 * 配り物に入るもの（`TR-PKG-28` の同梱物一覧）。
 *
 * 全部は並べない。 102 本の WAV を1行ずつ出しても読まないので、
 * 先頭のいくつかと、残りの数にまとめる。説明書の同梱物一覧も同じ形
 * （`koeru-package` の `contents_summary`）。
 *
 * 枠を持たない。 置く側の領域が枠になる（`docs/design/direction.md`）。
 */
export const PackageContents = ({ voiceId }: PackageContentsProps) => {
  const { data: files } = useSuspenseQuery(packageContentsQuery(voiceId));

  if (files.length === 0) {
    return <p className="text-sm text-slate-11">まだ入るものがありません。</p>;
  }

  const head = files.slice(0, ROOT_FILES);
  const rest = files.length - head.length;
  const total = files.reduce((sum, f) => sum + f.bytes, 0);

  return (
    <>
      <ul className="flex flex-col gap-1">
        {head.map((f) => (
          <li key={f.path} className="flex items-baseline justify-between gap-3 text-xs">
            <span className="min-w-0 truncate font-mono text-slate-12">{f.path}</span>
            <span className="shrink-0 font-mono text-slate-11 tabular-nums">
              {readableSize(f.bytes)}
            </span>
          </li>
        ))}
      </ul>

      {rest > 0 && (
        <p className="text-xs text-slate-11">
          ほかに <span className="font-mono tabular-nums">{rest}</span> 個のファイル。
        </p>
      )}

      <p className="text-xs text-slate-11">
        ぜんぶで <span className="font-mono tabular-nums">{readableSize(total)}</span> くらい。
      </p>
    </>
  );
};

/**
 * 人が読む大きさ。
 *
 * 単位を省かない（`docs/design/direction.md` の言葉の規律）。
 * 1024 で割る——ファイルの大きさは OS もそう出す。
 */
const readableSize = (bytes: number): string => {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${Math.round(kb)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
};
