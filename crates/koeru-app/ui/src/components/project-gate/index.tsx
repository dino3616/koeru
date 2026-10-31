import { useSuspenseQuery } from "@tanstack/react-query";
import { Suspense, type ReactNode } from "react";

import { Breath } from "~/components/breath";
import { openProjectQuery } from "~/lib/queries";

/**
 * 音源を開いてから中身を描く。
 *
 * 置くのは、場をまたいで描かれたままの層だけ。 `openProjectQuery` は観測者が
 * 外れた時点で捨てられ、次に付いたときに開き直す。場の画面ごとに置くと、
 * 収録から曲バンクへ移るだけで収録セッションが始め直しになる。外枠の層に
 * 置けば、場を移っても観測者が途切れない。音源を替えると鍵が変わり、そこで開き直す。
 *
 * 中身は開き終わるまで描かない。 台帳を読む子が先に走ると `app.no_project` を受ける。
 *
 * `id` が無ければ開かずに中身を出す。 殻を先に出す段階（prerender）と、
 * 音源を選ぶ前の画面（新しい音源、設定）がそれにあたる。
 */
export const ProjectGate = ({ id, children }: { id: string | undefined; children: ReactNode }) => {
  if (id === undefined) return <>{children}</>;

  return (
    <Suspense
      fallback={
        <main className="flex h-full flex-col items-center justify-center p-8">
          <p role="status" className="flex items-center gap-2 text-sm text-slate-11">
            <Breath size="sm" />
            開いています
          </p>
        </main>
      }
    >
      <Opened id={id}>{children}</Opened>
    </Suspense>
  );
};

const Opened = ({ id, children }: { id: string; children: ReactNode }) => {
  useSuspenseQuery(openProjectQuery(id));
  return <>{children}</>;
};
