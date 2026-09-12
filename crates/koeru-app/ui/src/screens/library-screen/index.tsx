import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { Suspense, useState } from "react";

import { AppMark } from "~/components/app-mark";
import { Breath } from "~/components/breath";
import { NewVoice } from "~/components/new-voice";
import { VoiceTile } from "~/components/voice-tile";
import { errorMessage } from "~/lib/ipc";
import { api } from "~/lib/ipc";
import { projectsQuery } from "~/lib/queries";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * 声の並び（`DEC-PLT-024`）。
 *
 * 一覧ではなく並び。 UTAU の音源はキャラクターとして記憶されている
 * （`EVID-UX-004`）。名前と数字の行に畳むと、その位置に何も座らない。
 *
 * 「作りかけ」の見出しを置かない。 完成しているかどうかは環が言う
 * （`DEC-PLT-025`、`DEC-PKG-007`）——被覆が満ちた瞬間には必ず完成しているので、
 * 別の印も、完成／未完成で分けた見出しも要らない。
 *
 * 書き出し・公開・作者の語を出さない（`TR-PKG-35`）。
 */
export const LibraryScreen = () => {
  const heading = useScreenFocus();
  const [creating, setCreating] = useState(false);

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <header className="flex h-16 flex-shrink-0 items-center gap-5 border-slate-6 border-b px-8">
        <AppMark />
      </header>

      <div className="flex-1 overflow-y-auto p-8">
        <div className="mx-auto flex w-full max-w-[1152px] flex-col gap-8">
          <div className="flex flex-col gap-2">
            <h1
              ref={heading}
              tabIndex={-1}
              className="text-xl font-semibold text-slate-12 outline-none"
            >
              声
            </h1>
            {/* 本文の段で置く。 補助の段（0.75rem）に落としていたので、
                この画面でいちばん小さい字が、製品の言いたいことになっていた。 */}
            <p className="text-sm text-slate-11">録っている途中でも、あなたの声が歌います。</p>
          </div>

          {/*
            並びを先に出す。 中身の取得を待たせない
            （`async-suspense-boundaries`）。まだ何も無い人にとっては、
            待つ意味のあるものが1つも無い画面になる。
          */}
          <Suspense
            fallback={
              <p role="status" className="flex items-center gap-2 py-8 text-sm text-slate-11">
                <Breath size="sm" />
                読み込んでいます
              </p>
            }
          >
            <Gallery creating={creating} onCreating={setCreating} />
          </Suspense>
        </div>
      </div>
    </main>
  );
};

/** 並びと、空いた席。失敗は上の `ErrorBoundary` が受ける。 */
const Gallery = ({
  creating,
  onCreating,
}: {
  creating: boolean;
  onCreating: (v: boolean) => void;
}) => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: projects } = useSuspenseQuery(projectsQuery());

  /**
   * 作って、その場で開く。
   *
   * 一覧の取り直しを待たずに移る。 待たせても、移った先の画面には
   * 一覧が出ていない——見えないものの更新のために足止めしない。
   */
  const create = useMutation({
    mutationFn: (displayName: string) => api.createProject(displayName),
    onSuccess: async (id) => {
      onCreating(false);
      void queryClient.invalidateQueries(projectsQuery());
      await navigate({ to: "/voice", search: { id, tab: "sound" } });
    },
  });

  if (creating) {
    return (
      <div className="flex justify-center py-8">
        <Suspense
          fallback={
            <p role="status" className="flex items-center gap-2 text-sm text-slate-11">
              <Breath size="sm" />
              読み込んでいます
            </p>
          }
        >
          <NewVoice
            creating={create.isPending}
            onCreate={(name) => create.mutate(name)}
            onClose={() => onCreating(false)}
          />
        </Suspense>
        {create.error !== null && (
          <p role="alert" className="text-sm text-red-11">
            {errorMessage(create.error)}
          </p>
        )}
      </div>
    );
  }

  return (
    <ul className="grid grid-cols-3 gap-8">
      <li>
        {/*
          空いた席。 座っていないことを、そのまま席として置く。
          「まだ何もありません」と書かない——**欠けを不足として書かない**
          （`docs/design/direction.md`）。

          先頭に置く。 末尾に置いていたので、声が3つを超えると
          席が次の段へ回り、**作りはじめるのにスクロールが要った。**
        */}
        <button
          type="button"
          onClick={() => onCreating(true)}
          className="flex size-full min-h-[280px] flex-col items-center justify-center gap-3 rounded-xl border border-slate-11 p-5 text-sm text-slate-11 hover:bg-slate-2 hover:text-slate-12"
        >
          <svg
            width="24"
            height="24"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <path d="M12 5v14M5 12h14" />
          </svg>
          新しく作る
        </button>
      </li>
      {projects.map((p) => (
        <li key={p.id}>
          <VoiceTile
            project={p}
            onOpen={() => void navigate({ to: "/voice", search: { id: p.id, tab: "sound" } })}
          />
        </li>
      ))}
    </ul>
  );
};
