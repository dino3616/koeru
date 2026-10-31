import { Link, useRouterState, useSearch } from "@tanstack/react-router";

import { SCREEN_NAMES, STUDIO_PATHS } from "~/lib/screen-names";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * 中身を作る前の画面。 経路が開くことと、どの経路かだけを示す。
 *
 * 題は経路から引く。 経路ごとに字を渡すと、読み上げの名前（`SCREEN_NAMES`）と
 * 画面の題が別々に書かれ、片方だけが変わる。
 *
 * 下の行き先は、シェルのサイドバーが来るまでの代わり。 Tauri の窓には
 * 戻るボタンが無いので、これが無いと一度入った経路から出られない。
 * `id` を持ち回す。 落とすと、移った先で音源が分からなくなる。 新しい音源にだけは
 * 渡さない。 渡すと、これから作る音源の画面で前の音源を開く。
 */
export const DraftScreen = () => {
  const heading = useScreenFocus();
  const path = useRouterState({ select: (s) => s.location.pathname });
  const { id } = useSearch({ strict: false });

  return (
    <main className="flex h-full flex-col gap-8 overflow-y-auto p-8">
      <div className="flex flex-col gap-2">
        <h1
          ref={heading}
          tabIndex={-1}
          className="text-xl font-semibold text-slate-12 outline-none"
        >
          {SCREEN_NAMES[path] ?? path}
        </h1>
        <p className="text-sm text-slate-11">作りかけ</p>
      </div>
      <nav aria-label="作りかけの画面">
        <ul className="flex flex-col gap-2">
          {STUDIO_PATHS.map((to) => (
            <li key={to}>
              <Link
                to={to}
                search={to === "/new" ? {} : { id }}
                className="text-sm text-slate-12 underline underline-offset-4 aria-[current=page]:font-semibold aria-[current=page]:no-underline"
              >
                {SCREEN_NAMES[to]}
              </Link>
            </li>
          ))}
          <li>
            <Link to="/" className="text-sm text-slate-12 underline underline-offset-4">
              今の画面へ戻る
            </Link>
          </li>
        </ul>
      </nav>
    </main>
  );
};
