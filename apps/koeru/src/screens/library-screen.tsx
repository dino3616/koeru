import { useNavigate } from "@tanstack/react-router";
import { useEffect, useState } from "react";

import { Button } from "~/components/ui/button";
import { Card, CardTitle } from "~/components/ui/card";
import { api, errorMessage, type ProjectView } from "~/lib/ipc";

/**
 * プロジェクトの一覧。
 *
 * **ここに書き出し・公開・作者の語を出さない**（TR-PKG-35）。
 * 完成しているかどうかだけを見せる。
 */
export const LibraryScreen = () => {
  const navigate = useNavigate();
  const [projects, setProjects] = useState<ProjectView[]>([]);
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const reload = () => {
    api
      .listProjects()
      .then(setProjects)
      .catch((e: unknown) => setError(errorMessage(e)));
  };

  useEffect(reload, []);

  const create = () => {
    const trimmed = name.trim();
    if (trimmed === "" || busy) return;
    setBusy(true);
    setError(null);
    api
      .createProject(trimmed)
      .then((id) => {
        setName("");
        reload();
        return navigate({ to: "/record", search: { id } });
      })
      .catch((e: unknown) => setError(errorMessage(e)))
      .finally(() => setBusy(false));
  };

  return (
    <main className="mx-auto flex h-full max-w-3xl flex-col gap-6 overflow-y-auto p-8">
      <header>
        <h1 className="text-2xl font-semibold">音源</h1>
        <p className="mt-1 text-sm text-text-dim">録音の途中でも、自分の声で歌を聴けます。</p>
      </header>

      <Card>
        <CardTitle>新しく作る</CardTitle>
        <div className="mt-3 flex gap-2">
          <label className="sr-only" htmlFor="new-name">
            音源の名前
          </label>
          <input
            id="new-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") create();
            }}
            placeholder="音源の名前"
            autoComplete="off"
            className="h-11 flex-1 select-text rounded-lg border border-border-strong bg-surface-2 px-3 text-sm text-text placeholder:text-text-dim"
          />
          <Button variant="primary" onClick={create} disabled={name.trim() === "" || busy}>
            作る
          </Button>
        </div>
        <p className="mt-2 text-xs text-text-dim">あとから変えられます。絵文字や記号も使えます。</p>
      </Card>

      {error !== null && (
        <p role="alert" className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text">
          {error}
        </p>
      )}

      <section className="flex flex-col gap-2">
        <CardTitle>作りかけ</CardTitle>
        {projects.length === 0 ? (
          <p className="py-8 text-center text-sm text-text-dim">まだ何もありません。</p>
        ) : (
          <ul className="flex flex-col gap-2">
            {projects.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  onClick={() => navigate({ to: "/record", search: { id: p.id } })}
                  className="flex w-full items-center justify-between rounded-xl border border-border bg-surface px-5 py-4 text-left hover:bg-surface-2"
                >
                  <span className="select-text font-medium">
                    {p.display_name ?? "（名前を読めませんでした）"}
                  </span>
                  <span className="font-mono text-xs text-text-dim tabular-nums">
                    {p.item_count ?? "?"} 項目
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
};
