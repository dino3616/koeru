import { Outlet, createFileRoute } from "@tanstack/react-router";
import * as v from "valibot";

import { ProjectGate } from "~/components/project-gate";
import { StudioShell } from "~/screens/studio-shell";

/**
 * 刷新した画面の外枠。 経路を持たず、場をまたいで描かれたまま残る。
 *
 * サイドバーとツールバーはここ（`StudioShell`）。 `<main>` は場の画面が持つので、
 * 外枠は包まない。
 *
 * 音源を開くのはここで1回だけ（`ProjectGate`）。 場の画面で開くと、
 * 場を移るたびに収録セッションが始め直しになる。 開く待ちはシェルの中身の
 * 領域で出す——外枠まで消すと、待つあいだ別の音源へ移れなくなる。
 *
 * 音源は検索引数で持つ。 子の経路はすべてこれを受け継ぐ。 子へ移るときは
 * `id` を渡し直す——落とすと、移った先は音源を選ぶ前の画面になる。
 *
 * `id` は省略できる。 省略を弾くと、殻を先に出す段階（prerender）でここを通れない。
 */
const searchSchema = v.object({
  id: v.optional(v.pipe(v.string(), v.uuid("音源の識別子が UUID ではない"))),
});

const Studio = () => {
  const { id } = Route.useSearch();
  return (
    <StudioShell id={id}>
      <ProjectGate id={id}>
        <Outlet />
      </ProjectGate>
    </StudioShell>
  );
};

export const Route = createFileRoute("/_studio")({
  validateSearch: searchSchema,
  component: Studio,
});
