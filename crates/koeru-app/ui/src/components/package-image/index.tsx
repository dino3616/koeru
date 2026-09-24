import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useMemo } from "react";

import { Button } from "~/components/button";
import { api, errorMessage } from "~/lib/ipc";
import { ledgerKey, packageImageQuery } from "~/lib/queries";

/** 受け付ける大きさの上限（バイト）。 */
const MAX_BYTES = 8 * 1024 * 1024;

type PackageImageProps = {
  voiceId: string;
  /** どちらの絵か。 */
  slot: "icon" | "portrait";
  label: string;
  hint: string;
};

/**
 * 配り物に入れる絵（`TR-PKG-07`、`DEC-PKG-012`）。
 *
 * アイコンは書き出しのときに 100×100 の BMP へ変換する。 ここが持つのは
 * 元画像で、変換後は持たない——寸法や畳み方を変えたときに作り直せなくなる。
 *
 * 選んだ時点で変換できるかを Rust が確かめる。 書き出しの一歩手前で
 * 「この画像は使えません」と言われないようにする。
 *
 * 枠を持たない。 置く側の領域が枠になる（`docs/reports/design/direction.md`）。
 */
export const PackageImage = ({ voiceId, slot, label, hint }: PackageImageProps) => {
  const inputId = useId();
  const queryClient = useQueryClient();
  const { data: bytes } = useQuery(packageImageQuery(voiceId, slot));

  /*
   * 見せるための URL を作る。
   *
   * 外部の仕組み（ブラウザの Blob URL）との同期なので、後始末を effect に
   * 置く（`react-conventions` の例外）。剥がさないと、選び直すたびに
   * 前の絵が参照されたまま残る。
   */
  const url = useMemo(
    () => (bytes === null || bytes === undefined ? null : blobUrl(bytes)),
    [bytes],
  );
  useEffect(() => {
    if (url === null) return;
    return () => URL.revokeObjectURL(url);
  }, [url]);

  const put = useMutation({
    mutationFn: async (next: number[] | null) => {
      if (slot === "icon") {
        await api.setPackageIcon(next);
        return;
      }
      await api.setPackagePortrait(next);
    },
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  const pick = useMutation({
    mutationFn: async (file: File) => {
      if (file.size > MAX_BYTES) {
        throw new Error("画像が大きすぎます。8 MB までにしてください。");
      }
      return Array.from(new Uint8Array(await file.arrayBuffer()));
    },
    onSuccess: (next) => put.mutate(next),
  });

  const chosen = bytes !== null && bytes !== undefined;
  const busy = pick.isPending || put.isPending;
  const failure = pick.error ?? put.error;

  return (
    <div className="flex flex-col gap-2">
      <label className="text-xs text-slate-11" htmlFor={inputId}>
        {label}
      </label>

      <div className="flex items-center gap-3">
        {url === null ? (
          <span
            aria-hidden="true"
            className="flex h-16 w-16 shrink-0 items-center justify-center rounded-lg border border-dashed border-slate-7 bg-slate-3 text-xs text-slate-11"
          >
            なし
          </span>
        ) : (
          <img
            src={url}
            alt={`${label}に選んである絵`}
            className="h-16 w-16 shrink-0 rounded-lg border border-slate-7 object-cover"
          />
        )}

        <input
          id={inputId}
          type="file"
          accept="image/png,image/jpeg"
          disabled={busy}
          onChange={(e) => {
            const file = e.target.files?.[0];
            // 同じ絵を選び直せるようにする。値を残すと、2回目の change が飛ばない。
            e.target.value = "";
            if (file !== undefined) pick.mutate(file);
          }}
          className="min-w-0 flex-1 select-text text-sm text-slate-12 file:mr-3 file:h-9 file:rounded-lg file:border file:border-slate-7 file:bg-slate-3 file:px-3 file:text-sm file:text-slate-12"
        />
      </div>

      <p className="text-xs text-slate-11">{hint}</p>

      {chosen && (
        <Button
          variant="ghost"
          size="sm"
          className="self-start"
          disabled={busy}
          onClick={() => put.mutate(null)}
        >
          外す
        </Button>
      )}

      {failure !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(failure)}
        </p>
      )}
    </div>
  );
};

/** バイト列を、画面に出せる URL にする。 */
const blobUrl = (bytes: number[]) => URL.createObjectURL(new Blob([new Uint8Array(bytes)]));
