import { useSuspenseQuery } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import type { RowTakesView } from "~/lib/ipc";
import { preflightQuery } from "~/lib/queries";

type PackagePanelProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
  rows: readonly RowTakesView[];
  /** その行の録った回へ入る（`TR-PKG-51` の横移動）。 */
  onOpenRow: (rowId: string) => void;
};

/**
 * 配り物（`DEC-PLT-024`、`TR-PKG-35`）。
 *
 * **書き出しはまだ無い**（`PROFILE-M4`）。 面だけ先に置いて、
 * いま出せることだけを出す。作れないものを灰色の的として並べない——
 * 押せない的は、押せば何か起きると思わせる。
 *
 * 配ることを必須にしない（`TR-PKG-35`）。 「公開」「配布」「作者」「規約」を
 * 通常モードの必須ステップとして出さない。非公開のまま完成できる。
 *
 * 見つかったことから、その行へ直接入れる（`TR-PKG-51`）。
 * コレクションを経由しない横移動。
 */
export const PackagePanel = ({ voiceId, rows, onOpenRow }: PackagePanelProps) => {
  const { data: preflight } = useSuspenseQuery(preflightQuery(voiceId));
  const textOf = (rowId: string) => rows.find((r) => r.row_id === rowId)?.text ?? "この行";
  const findings = preflight.clipped_takes.length + preflight.non_nfc_names.length;

  return (
    <Card title="書き出す前に見ること">
      <p className="font-mono text-xs text-slate-11 tabular-nums">{findings} 件</p>

      {findings === 0 ? (
        <p className="text-sm text-slate-12">いまのところ、引っかかるものはありません。</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {preflight.clipped_takes.map(([rowId, times]) => (
            <li key={rowId} className="flex gap-3">
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                stroke="var(--amber-11)"
                strokeWidth="1.6"
                strokeLinecap="round"
                className="mt-1 flex-shrink-0"
                aria-hidden="true"
              >
                <path d="M8 2.5 14.5 13.5h-13z" />
                <path d="M8 6.5v3M8 11.6v.1" />
              </svg>
              <span className="flex min-w-0 flex-col gap-2">
                <span className="select-text text-sm text-slate-12">
                  「{textOf(rowId)}」で音が {times} 回いちばん上まで届いています。
                </span>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => onOpenRow(rowId)}
                  aria-label={`${textOf(rowId)} の録った回を見る`}
                >
                  見る
                </Button>
              </span>
            </li>
          ))}

          {preflight.non_nfc_names.length > 0 && (
            <li className="flex gap-3">
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                stroke="var(--red-11)"
                strokeWidth="1.6"
                strokeLinecap="round"
                className="mt-1 flex-shrink-0"
                aria-hidden="true"
              >
                <circle cx="8" cy="8" r="6" />
                <path d="M8 5v4M8 11.2v.1" />
              </svg>
              <span className="text-sm text-slate-12">
                受け取った人の環境で見つからなくなる名前が{" "}
                <span className="font-mono tabular-nums">{preflight.non_nfc_names.length}</span>{" "}
                件あります。この状態では書き出せません。
              </span>
            </li>
          )}
        </ul>
      )}

      {preflight.renamed_to_nfc > 0 && (
        <p className="text-xs text-slate-11">
          <span className="font-mono tabular-nums">{preflight.renamed_to_nfc}</span>{" "}
          件の名前は、受け取る側で読める形に直してあります。
        </p>
      )}
    </Card>
  );
};
