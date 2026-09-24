import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { api, errorMessage } from "~/lib/ipc";
import { presampNoticeQuery } from "~/lib/queries";

type PresampNoticeProps = {
  /** 開いている声。 開いたあとでなければ読めない。 */
  voiceId: string;
};

/**
 * 書き換えられていた綴りの表を戻したことの知らせ（`DEC-SYN-013`）。
 *
 * 綴りの表は作るときに選んで固定する。 開いたときに声のフォルダの
 * `presamp.ini` が書き換えられていれば、作ったときの中身へ戻し、
 * 書き換えられた中身は別の名前で残してある。**黙って戻さない**——
 * 本人が書いたものなので、どこへ行ったかを伝える。
 *
 * 置き場所は出さない（`TR-PKG-45`）。 残したファイルの名前だけを出す。
 *
 * 中断させない。 知らせが読めなくても、声の面は成り立つ。
 */
export const PresampNotice = ({ voiceId }: PresampNoticeProps) => {
  const queryClient = useQueryClient();
  const { data: kept } = useQuery(presampNoticeQuery(voiceId));
  const dismiss = useMutation({
    mutationFn: () => api.dismissPresampNotice(),
    onSuccess: () => void queryClient.invalidateQueries(presampNoticeQuery(voiceId)),
  });

  if (kept === undefined || kept === null) return null;

  return (
    <div
      role="status"
      className="flex flex-col gap-2 rounded-lg border border-slate-7 bg-slate-3 p-3"
    >
      <p className="text-sm text-slate-12">
        綴りの表（presamp.ini）が書き換えられていたので、この声を作ったときの中身に戻しました。
      </p>
      <p className="text-xs text-slate-11">
        書き換えられていた中身は、声のフォルダに「
        <span className="font-mono select-text">{kept}</span>
        」として残してあります。綴りの表は、声を作るときにだけ選べます。
      </p>
      <Button
        variant="secondary"
        size="sm"
        className="self-start"
        onClick={() => dismiss.mutate()}
        disabled={dismiss.isPending}
      >
        分かった
      </Button>
      {dismiss.error !== null && (
        <p role="alert" className="text-xs text-red-11">
          {errorMessage(dismiss.error)}
        </p>
      )}
    </div>
  );
};
