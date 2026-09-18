import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { Field } from "~/components/field";
import { api, errorMessage } from "~/lib/ipc";
import { ledgerKey, packageStateQuery } from "~/lib/queries";

type PackageExportProps = {
  voiceId: string;
};

/**
 * 書き出す（`REQ-PKG-105`, `REQ-PKG-106`, `DEC-PKG-010`）。
 *
 * 検証を通っていない間は押せない（`FB-PKG-102`）。 押せない的を灰色で
 * 置いているのは、何を直せば押せるようになるかが上の面に出ているため。
 *
 * 2つ出る。 どこでも開ける ZIP と、UTAU 本体へ落とせる UAR（`DEC-PKG-010`）。
 * 中身は同じ。
 *
 * 場所は出さない（`TR-PKG-45`）。 保存先もファイル名も見せず、
 * 「できました」とだけ言う。
 */
export const PackageExport = ({ voiceId }: PackageExportProps) => {
  const versionId = useId();
  const queryClient = useQueryClient();
  const { data: state } = useSuspenseQuery(packageStateQuery(voiceId));
  const [version, setVersion] = useState("");

  const run = useMutation({
    mutationFn: (v: string) => api.exportPackage(v),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  const ready = state.may_export && !run.isPending;

  return (
    <Card title="配り物をつくる">
      <Field
        id={versionId}
        label="この回の呼び名"
        hint="空でも作れます。あとで見分けるための札です。"
        value={version}
        onChange={(e) => setVersion(e.target.value)}
      />

      <p className="text-sm text-slate-12">
        <span className="font-mono tabular-nums">{state.file_count}</span> 個のファイルと{" "}
        <span className="font-mono tabular-nums">{state.alias_count}</span> 個の呼び名が入ります。
      </p>

      <Button variant="primary" onClick={() => run.mutate(version.trim())} disabled={!ready}>
        {run.isPending ? "つくっています" : "つくる"}
      </Button>

      {!state.may_export && (
        <p className="text-xs text-slate-11">
          上に出ていることを直すと、ここから作れるようになります。
        </p>
      )}

      {run.data !== undefined && (
        <p className="rounded-lg bg-slate-3 px-3 py-2 text-sm text-slate-12">
          できました。
          <span className="select-text font-mono"> {run.data.archive_name}</span> と、同じ中身の UAR
          を書き出しました。
        </p>
      )}

      {run.error !== null && (
        <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
          {errorMessage(run.error)}
        </p>
      )}
    </Card>
  );
};
