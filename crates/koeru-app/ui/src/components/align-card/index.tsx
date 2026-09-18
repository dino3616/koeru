import { useSuspenseQueries } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { modelNoticeQuery, staleTakesQuery } from "~/lib/queries";

type AlignCardProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
  /** その行の録った回へ入る。 */
  onOpenRow: (rowId: string) => void;
  /** 行 ID から読み上げ文字列を引く。行 ID は画面に出さない（`DEC-REC-009`）。 */
  textOf: (rowId: string) => string;
};

/**
 * 自動で決めた切り出しの出どころ（`TR-ALN-29`, `TR-ALN-31`）。
 *
 * **モデルが変わっても作り直さない。** アプリを更新したら、昨日確認し終えた
 * 切り出しが全部作り直されている、は事故（`TR-ALN-29`）。ここが出すのは
 * 「前の版で作られたものがある」という事実だけで、録り直すかは本人が決める。
 *
 * 帰属表示をアプリの中に置く（`TR-ALN-31`）。 同梱しているモデルは CC BY 系で、
 * 帰属表示・ライセンス・変更の明示が義務（`DEC-ALN-008`）。台帳に留めると
 * 義務を果たしたことにならない。
 *
 * 既定では畳んでおく。 収録している最中に読ませるものではない。
 */
export const AlignCard = ({ voiceId, onOpenRow, textOf }: AlignCardProps) => {
  const [{ data: stale }, { data: notice }] = useSuspenseQueries({
    queries: [staleTakesQuery(voiceId), modelNoticeQuery()],
  });
  const [open, setOpen] = useState(false);

  return (
    <Card title="切り出しの出どころ">
      {stale.length === 0 ? (
        <p className="text-sm text-slate-12">切り出しは、いま入っているモデルで作られています。</p>
      ) : (
        <>
          <p className="text-sm text-slate-12">
            {stale.length} 行の切り出しは、前の版のモデルで作られています。 そのままでも使えます。
          </p>
          <ul className="flex flex-col gap-2">
            {stale.map((rowId) => (
              <li key={rowId} className="flex items-center justify-between gap-3">
                <span className="select-text text-sm text-slate-12">{textOf(rowId)}</span>
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => onOpenRow(rowId)}
                  aria-label={`${textOf(rowId)} の録った回を見る`}
                >
                  見る
                </Button>
              </li>
            ))}
          </ul>
        </>
      )}

      <div className="flex flex-col items-start gap-3">
        <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setOpen(!open)}>
          同梱しているモデル
        </Button>
        {open && (
          /*
            中で改めてスクロールさせない。 押せるものを持たない領域を
            スクロールさせると、キーボードだけでは下まで読めなくなる
            （`TR-PLT-26`）。面ごと伸ばして、外側のスクロールに任せる。
          */
          <pre className="w-full select-text whitespace-pre-wrap font-mono text-xs text-slate-11">
            {notice}
          </pre>
        )}
      </div>
    </Card>
  );
};
