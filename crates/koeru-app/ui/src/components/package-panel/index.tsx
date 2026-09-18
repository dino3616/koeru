import { useSuspenseQueries } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import type { RowTakesView } from "~/lib/ipc";
import { findingLabel, placeLabel } from "~/lib/labels";
import { packageStateQuery, preflightQuery } from "~/lib/queries";

type PackagePanelProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
  rows: readonly RowTakesView[];
  /** その行の録った回へ入る（`TR-PKG-51` の横移動）。 */
  onOpenRow: (rowId: string) => void;
};

/**
 * 書き出す前に見ること（`TR-PKG-49`, `TR-PKG-50`, `TR-PKG-17`, `TR-PKG-51`）。
 *
 * 1件でも止まるものが残っていれば書き出さない（`REQ-PKG-104`）。
 * 割れているテイクだけは止めない——本人が承知のうえで配ることはある。
 *
 * どこを直せばよいかまで出す（`TR-PKG-51`）。 ファイル名と行番号だけを
 * 出して終わらない。見つかった行へは、ここから1操作で入れる。
 *
 * 原音設定の確認は隣の `ReviewPanel` が持つ（`TR-ALN-21`）。 あちらは
 * 確認が済んだかを関門にしていて（`INV-ALN-003`）、ここが見る名前・割れ・
 * 符号化とは別の条件で止まる。
 *
 * 配ることを必須にしない（`TR-PKG-35`）。 「公開」「配布」「作者」「規約」を
 * 通常モードの必須ステップとして出さない。非公開のまま完成できる。
 */
export const PackagePanel = ({ voiceId, rows, onOpenRow }: PackagePanelProps) => {
  /*
   * まとめて並行に取る。
   *
   * `useSuspenseQuery` を並べない。 同じ部品に並べると**直列**になる
   * （`EVID-PLT-001` で実測）。どちらも台帳と WAV を読むので、
   * 直列にすると待ちが倍になる。
   */
  const [{ data: preflight }, { data: state }] = useSuspenseQueries({
    queries: [preflightQuery(voiceId), packageStateQuery(voiceId)],
  });

  const textOf = (rowId: string) => rows.find((r) => r.row_id === rowId)?.text ?? "この行";
  const stopping =
    state.findings.length + state.unencodable.length + preflight.non_nfc_names.length;
  const total = stopping + preflight.clipped_takes.length;

  return (
    <Card title="書き出す前に見ること">
      <p className="font-mono text-xs text-slate-11 tabular-nums">{total} 件</p>

      {total === 0 ? (
        <p className="text-sm text-slate-12">いまのところ、引っかかるものはありません。</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {/* 書けない文字（`TR-PKG-17`）。勝手に置き換えないので、本人が直す。 */}
          {state.unencodable.map((u) => (
            <li key={`${u.place}-${u.target ?? ""}`} className="flex gap-3">
              <Stop />
              <span className="flex min-w-0 flex-col gap-2">
                <span className="select-text text-sm text-slate-12">
                  {`${placeLabel(u.place)}${u.target === null ? "" : `「${u.target}」`}に、この形では書けない字があります。`}
                </span>
                <span className="select-text font-mono text-sm text-slate-12">
                  {u.chars.join(" ")}
                </span>
                {u.suggestion !== null && (
                  <span className="select-text text-xs text-slate-11">
                    たとえば「{u.suggestion}」にすると書けます。
                  </span>
                )}
                {u.row_id !== null && (
                  <Open label={textOf(u.row_id)} onClick={() => onOpenRow(u.row_id ?? "")} />
                )}
              </span>
            </li>
          ))}

          {/* 検証で止まるもの（`TR-PKG-49`）。 */}
          {state.findings.map((f, i) => (
            <li key={`${f.file}-${f.alias ?? ""}-${f.kind}-${i}`} className="flex gap-3">
              <Stop />
              <span className="flex min-w-0 flex-col gap-2">
                <span className="select-text text-sm text-slate-12">
                  {findingLabel(f.kind)}
                  {f.alias === null ? "" : `（呼び名「${f.alias}」）`}
                </span>
                {f.row_id !== null && (
                  <Open label={textOf(f.row_id)} onClick={() => onOpenRow(f.row_id ?? "")} />
                )}
              </span>
            </li>
          ))}

          {/* 止めないもの（`TR-PKG-49` の関門、`TR-REC-16`）。 */}
          {preflight.clipped_takes.map(([rowId, times]) => (
            <li key={rowId} className="flex gap-3">
              <Warn />
              <span className="flex min-w-0 flex-col gap-2">
                <span className="select-text text-sm text-slate-12">
                  「{textOf(rowId)}」で音が {times} 回いちばん上まで届いています。
                </span>
                <Open label={textOf(rowId)} onClick={() => onOpenRow(rowId)} />
              </span>
            </li>
          ))}

          {preflight.non_nfc_names.length > 0 && (
            <li className="flex gap-3">
              <Stop />
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

/** 止まるもの。色だけで伝えないので、文面の側に「書き出せません」を書く。 */
const Stop = () => (
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
);

/** 止めないもの。 */
const Warn = () => (
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
);

/** その行の録った回へ入る（`TR-PKG-51`）。 */
const Open = ({ label, onClick }: { label: string; onClick: () => void }) => (
  <Button
    variant="secondary"
    size="sm"
    className="self-start"
    onClick={onClick}
    aria-label={`${label} の録った回を見る`}
  >
    見る
  </Button>
);
