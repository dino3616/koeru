/**
 * KOERU のしるし。
 *
 * **形はまだ決まっていない**（`Q-PLT-005`）。 すれ違う二つの弧を仮に置いてある。
 * 求めているのは「声の気配がすること」で、意味の正確さでも覚えやすさでもない。
 * 決まったら、ここの `path` だけを差し替える。
 *
 * 環を使わない（`DEC-PLT-025`）。 環は本人の声の形の語彙なので、
 * ブランドが使うと、ブランドの形と本人の声の形が同じ列で混ざる。
 * 共有しているのは線質だけ。
 *
 * ワードマークは KOERU のラテンのみ。 リポジトリ名・パッケージ名・
 * コマンド名と揃う。
 */
export const AppMark = () => (
  <span className="flex items-center gap-3">
    <svg
      width="24"
      height="24"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      aria-hidden="true"
      className="text-slate-12"
    >
      <path d="M5 19C5 11 9 5 15 5" />
      <path d="M19 5c0 8-4 14-10 14" />
    </svg>
    <span className="text-sm font-semibold tracking-[0.22em] text-slate-12">KOERU</span>
  </span>
);
