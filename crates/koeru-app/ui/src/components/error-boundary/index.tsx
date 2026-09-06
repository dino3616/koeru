import { Component, type ErrorInfo, type ReactNode } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { errorMessage } from "~/lib/ipc";

type Props = {
  children: ReactNode;
  /**
   * やり直す前に呼ぶ。
   *
   * 問い合わせの失敗を消すのはここ。 消さずに描き直すと、
   * 失敗したままの問い合わせをもう一度読んで即座に同じ例外が飛ぶ——
   * 「やり直す」を押しても画面が変わらない。
   */
  onReset?: () => void;
};

/**
 * 捕まえた例外。
 *
 * 値そのものではなく箱で持つ。 Rust 側の失敗は `Error` ではなく
 * `{ kind, message }` の素のオブジェクトで飛んでくるので、
 * 「例外が無い」を `null` で表すと `null` を投げられたときに区別できない。
 */
type State = { caught: { error: unknown } | null };

/**
 * 描画中の例外を受け止める。
 *
 * **白い画面にしない。** これが無いと、描画のどこかで例外が出た時点で
 * React が木を丸ごと外し、利用者には何も出ないまま戻る手段も無くなる。
 *
 * 収録の途中で起きうるので、やり直す手段をその場に置く。
 */
export class ErrorBoundary extends Component<Props, State> {
  override state: State = { caught: null };

  static getDerivedStateFromError(error: unknown): State {
    return { caught: { error } };
  }

  override componentDidCatch(error: unknown, info: ErrorInfo) {
    // 送信層へは載せない（`AGENTS.md` の禁止事項3）。開発中に手元で読むためだけ。
    console.error("描画で例外が出た", error, info.componentStack);
  }

  override render() {
    const { caught } = this.state;
    if (caught === null) return this.props.children;

    return (
      <main className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-4 p-8">
        {/*
          `role="alert"` にする（`TR-PLT-29`）。 常設の読み上げ領域は
          `aria-live="polite"` なので、割り込んでまでは読まれない。
          画面が丸ごと入れ替わったことは、待たせずに伝える。
        */}
        <Card title="画面を描けませんでした">
          <p role="alert" className="mt-3 text-sm text-slate-11">
            録れたものは失われていません。やり直しても直らないときは、
            この文言を添えて報告してください。
          </p>
          <p className="mt-3 select-text rounded-lg bg-slate-3 px-4 py-3 font-mono text-xs text-slate-11">
            {errorMessage(caught.error)}
          </p>
          <div className="mt-4 flex gap-2">
            <Button
              variant="primary"
              onClick={() => {
                this.props.onReset?.();
                this.setState({ caught: null });
              }}
            >
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
