import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Suspense, useState } from "react";

import { Button } from "~/components/button";
import { Card, CardTitle } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";
import { projectsQuery } from "~/lib/queries";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * プロジェクトの一覧。
 *
 * ここに書き出し・公開・作者の語を出さない（`TR-PKG-35`）。
 * 完成しているかどうかだけを見せる。
 */
export const LibraryScreen = () => {
  const navigate = useNavigate();
  const heading = useScreenFocus();
  const queryClient = useQueryClient();
  const [name, setName] = useState("");

  /**
   * 作って、その場で開く。
   *
   * 一覧の取り直しを待たずに移る。 待たせても、移った先の画面には
   * 一覧が出ていない——見えないものの更新のために足止めしない。
   */
  const create = useMutation({
    mutationFn: (displayName: string) => api.createProject(displayName),
    onSuccess: async (id) => {
      setName("");
      void queryClient.invalidateQueries(projectsQuery());
      await navigate({ to: "/record", search: { id } });
    },
  });

  const submit = () => {
    const trimmed = name.trim();
    if (trimmed === "" || create.isPending) return;
    create.mutate(trimmed);
  };

  return (
    <main className="mx-auto flex h-full max-w-3xl flex-col gap-6 overflow-y-auto p-8">
      <header>
        <h1 ref={heading} tabIndex={-1} className="text-2xl font-semibold outline-none">
          音源
        </h1>
        <p className="mt-1 text-sm text-slate-11">録音の途中でも、自分の声で歌を聴けます。</p>
      </header>

      <Card title="新しく作る">
        <div className="mt-3 flex gap-2">
          <label className="sr-only" htmlFor="new-name">
            音源の名前
          </label>
          <input
            id="new-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              // 変換確定の Enter で送信しない。 IME で変換しているあいだも
              // `keydown` は `Enter` で飛ぶので、見ないと変換途中の名前で作ってしまう。
              if (e.key === "Enter" && !e.nativeEvent.isComposing) submit();
            }}
            placeholder="音源の名前"
            autoComplete="off"
            className="h-11 flex-1 select-text rounded-lg border border-slate-11 bg-slate-3 px-3 text-sm text-slate-12 placeholder:text-slate-11"
          />
          <Button
            variant="primary"
            onClick={submit}
            disabled={name.trim() === "" || create.isPending}
          >
            作る
          </Button>
        </div>
        <p className="mt-2 text-xs text-slate-11">あとから変えられます。絵文字や記号も使えます。</p>
      </Card>

      {create.error !== null && (
        <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
          {errorMessage(create.error)}
        </p>
      )}

      <section className="flex flex-col gap-2" aria-labelledby="wip">
        <CardTitle id="wip">作りかけ</CardTitle>
        {/*
          見出しと「新しく作る」を先に出す。 一覧の取得を待たせない
          （`async-suspense-boundaries`）。まだ何も無い人にとっては、
          待つ意味のあるものが1つも無い画面になる。
        */}
        <Suspense
          fallback={
            <p role="status" className="py-8 text-center text-sm text-slate-11">
              読み込み中
            </p>
          }
        >
          <ProjectList />
        </Suspense>
      </section>
    </main>
  );
};

/** 作りかけの一覧。失敗は上の `ErrorBoundary` が受ける。 */
const ProjectList = () => {
  const navigate = useNavigate();
  const { data: projects } = useSuspenseQuery(projectsQuery());

  if (projects.length === 0) {
    return <p className="py-8 text-center text-sm text-slate-11">まだ何もありません。</p>;
  }

  return (
    <ul className="flex flex-col gap-2">
      {projects.map((p) => (
        <li key={p.id}>
          <button
            type="button"
            onClick={() => navigate({ to: "/record", search: { id: p.id } })}
            className="flex w-full items-center justify-between rounded-xl border border-slate-6 bg-slate-2 px-5 py-4 text-left hover:bg-slate-3"
          >
            <span className="select-text font-medium">
              {p.display_name ?? "（名前を読めませんでした）"}
            </span>
            <span className="font-mono text-xs text-slate-11 tabular-nums">
              {p.item_count ?? "?"} 項目
            </span>
          </button>
        </li>
      ))}
    </ul>
  );
};
