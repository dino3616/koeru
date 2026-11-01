import type { ReactNode } from "react";

/**
 * シェルが使う絵。 20px の枠に、線の太さ 1.6 で描く。
 *
 * 絵は `aria-hidden`。 意味は隣の字か、押せるものの名前が持つ。
 * 色は持たず、字と同じ色（`currentColor`）で描く。
 */
export type GlyphName =
  | "search"
  | "plus"
  | "chevron-down"
  | "chevron-right"
  | "sidebar"
  | "back"
  | "forward"
  | "home"
  | "mic"
  | "music"
  | "wave"
  | "package";

const GLYPHS: Record<GlyphName, ReactNode> = {
  search: (
    <>
      <circle cx="9" cy="9" r="5.5" />
      <path d="M13.2 13.2 17 17" />
    </>
  ),
  plus: <path d="M10 4v12M4 10h12" />,
  "chevron-down": <path d="m5 7.5 5 5 5-5" />,
  "chevron-right": <path d="m7.5 5 5 5-5 5" />,
  sidebar: (
    <>
      <rect x="2.5" y="4" width="15" height="12" rx="2.5" />
      <path d="M7.5 4v12" />
    </>
  ),
  back: <path d="M12 4.5 6.5 10l5.5 5.5" />,
  forward: <path d="M8 4.5 13.5 10 8 15.5" />,
  home: <path d="M3.5 9 10 3.5 16.5 9v7a.5.5 0 0 1-.5.5h-3.5V12h-5v4.5H4a.5.5 0 0 1-.5-.5z" />,
  mic: (
    <>
      <rect x="7.5" y="2.5" width="5" height="9" rx="2.5" />
      <path d="M4.5 9.5a5.5 5.5 0 0 0 11 0M10 15v2.5" />
    </>
  ),
  music: (
    <>
      <path d="M7.5 14.5V5l8-1.5v9.5" />
      <circle cx="5.5" cy="14.5" r="2" />
      <circle cx="13.5" cy="13" r="2" />
    </>
  ),
  wave: <path d="M2.5 10h2l2-5 3 10 3-8 1.5 3h3.5" />,
  package: (
    <>
      <path d="M10 2.5 16.5 6v8L10 17.5 3.5 14V6z" />
      <path d="M3.5 6 10 9.5 16.5 6M10 9.5v8" />
    </>
  ),
};

export const Glyph = ({ name }: { name: GlyphName }) => (
  <svg
    width="20"
    height="20"
    viewBox="0 0 20 20"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.6"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
    className="shrink-0"
  >
    {GLYPHS[name]}
  </svg>
);
