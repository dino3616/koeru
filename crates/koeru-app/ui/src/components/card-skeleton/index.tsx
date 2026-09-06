import { Card } from "~/components/card";
import { Spinner } from "~/components/spinner";

type CardSkeletonProps = {
  /** 何を待っているか。見出しにそのまま出す。 */
  title: string;
};

/**
 * 読み込み中の面。
 *
 * `Suspense` の受け皿に使う。 中身が来る前でも枠と見出しは出るので、
 * 待っている間に画面が飛ばない（`async-suspense-boundaries`）。
 *
 * 待ちは `role="status"` で伝える。 `Spinner` 自身は `aria-hidden` なので、
 * 画面を見ていない人にはここの文言だけが届く（`TR-PLT-29`）。
 */
export const CardSkeleton = ({ title }: CardSkeletonProps) => (
  <Card title={title}>
    <p role="status" className="mt-3 flex items-center gap-2 text-sm text-slate-11">
      <Spinner />
      読み込んでいます
    </p>
  </Card>
);
