import { useNavigate } from "@tanstack/react-router";

import { Button } from "~/components/ui/button";
import { Card } from "~/components/ui/card";
import { errorMessage } from "~/lib/ipc";

/**
 * ルータが投げた失敗の面。
 *
 * `ErrorBoundary` が拾うのは描画中の例外だけで、ルータ自身が起こした失敗は
 * その外を通る。 収録の途中で起きうるので、どちらの経路でもやり直す手段を
 * その場に置く（`TR-PLT-29`）。
 *
 * 原因は `errorMessage` を通す。 素の例外をそのまま出すと、
 * パスや音源名が画面に出る（`TR-PKG-45`）。
 */
export const RouteError = ({ error }: { error: unknown }) => {
  const navigate = useNavigate();
  return (
    <main className="mx-auto flex h-full max-w-3xl flex-col items-center justify-center p-8">
      <Card title="画面を開けませんでした">
        <p role="alert" className="mt-3 text-sm text-red-11">
          {errorMessage(error)}
        </p>
        <div className="mt-4 flex gap-3">
          <Button variant="primary" onClick={() => navigate({ to: "/" })}>
            一覧へ戻る
          </Button>
          <Button onClick={() => window.location.reload()}>やり直す</Button>
        </div>
      </Card>
    </main>
  );
};
