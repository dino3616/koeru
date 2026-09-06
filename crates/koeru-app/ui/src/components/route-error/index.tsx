import { useNavigate } from "@tanstack/react-router";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { errorMessage } from "~/lib/ipc";

/**
 * ルータが投げた失敗の面。
 *
 * 経路の中で起きた失敗はここが受ける。 ルータは経路ごとに自前の
 * 受け口を挟むので、`__root` の `ErrorBoundary` より内側で先に捕まる——
 * 取得の失敗（`useSuspenseQuery`）も、経路の中で投げられればここへ来る。
 * `ErrorBoundary` はその外側の backstop。収録の途中で起きうるので、
 * どちらの経路でもやり直す手段をその場に置く（`TR-PLT-29`）。
 *
 * 「やり直す」は読み込み直し。 問い合わせの失敗を抱えたまま描き直すと、
 * 同じ例外がその場で飛び直す。
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
