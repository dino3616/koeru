import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "~/components/ui/button";
import { Card } from "~/components/ui/card";

type Props = { children: ReactNode };
type State = { error: Error | null };

/**
 * 描画中の例外を受け止める。
 *
 * **白い画面にしない。** これが無いと、描画のどこかで例外が出た時点で
 * React が木を丸ごと外し、利用者には何も出ないまま戻る手段も無くなる。
 *
 * 収録の途中で起きうるので、やり直す手段をその場に置く。
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    // 送信層へは載せない（`AGENTS.md` の禁止事項3）。開発中に手元で読むためだけ。
    console.error("描画で例外が出た", error, info.componentStack);
  }

  override render() {
    const { error } = this.state;
    if (error === null) return this.props.children;

    return (
      <main className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-4 p-8">
        <Card title="画面を描けませんでした">
          <p className="mt-3 text-sm text-slate-11">
            録れたものは失われていません。やり直しても直らないときは、
            この文言を添えて報告してください。
          </p>
          <p className="mt-3 select-text rounded-lg bg-slate-3 px-4 py-3 font-mono text-xs text-slate-11">
            {error.message}
          </p>
          <div className="mt-4 flex gap-2">
            <Button variant="primary" onClick={() => this.setState({ error: null })}>
              やり直す
            </Button>
            <Button variant="ghost" onClick={() => window.location.assign("/")}>
              一覧へ戻る
            </Button>
          </div>
        </Card>
      </main>
    );
  }
}
