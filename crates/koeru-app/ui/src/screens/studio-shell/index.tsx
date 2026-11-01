import { useQuery } from "@tanstack/react-query";
import type { ReactNode } from "react";

import { AppShell } from "~/components/app-shell";
import { openProjectQuery, projectsQuery, subbanksQuery } from "~/lib/queries";

/**
 * 外枠に音源の棚と枠の木を渡す。
 *
 * どちらも `useQuery` で読む。 棚と木が読めなくても、中身の画面は成り立つ
 * ——`useSuspenseQuery` にすると、棚を待つあいだ外枠ごと消えて、
 * 場の画面も出られなくなる。 待ちと失敗は外枠が自分で描く。
 *
 * **枠は音源を開いたあとでなければ読めない**（`TR-REC-30`）。 開くのは
 * `ProjectGate` だけで、ここでは開かない。 同じ鍵を `enabled: false` で
 * 見るだけにして、開き終わったら読む。 `useSuspenseQuery(openProjectQuery)` で
 * 見ると、この観測者が付くたびに（サイドバーを開き直すたびに）取り直しが走り、
 * 収録セッションが始め直しになる。 あの問い合わせは `staleTime: 0`。
 */
export const StudioShell = ({ id, children }: { id: string | undefined; children: ReactNode }) => {
  const projects = useQuery(projectsQuery());
  // `id` が無いときは鍵に何も入れない。 開かない音源の鍵を作らせない。
  const opened =
    useQuery({ ...openProjectQuery(id ?? ""), enabled: false }).isSuccess && id !== undefined;
  const subbanks = useQuery({ ...subbanksQuery(id ?? ""), enabled: opened });

  return (
    <AppShell
      voices={
        projects.data?.map((p) => ({
          id: p.id,
          name: p.display_name ?? "",
          color: p.state?.color ?? null,
        })) ?? null
      }
      voicesFailed={projects.isError}
      selectedId={id}
      subbanks={
        subbanks.data?.map((s) => ({
          id: s.id,
          voiceColor: s.voiceColor,
          method: s.method,
          tone: s.tone.name,
        })) ?? null
      }
      subbanksFailed={subbanks.isError}
    >
      {children}
    </AppShell>
  );
};
