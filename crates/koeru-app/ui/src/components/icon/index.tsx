import type { ReactNode } from "react";

/**
 * 印の名前。 形はどれも刷新のボード（`docs/reports/ux/redesign-canvas/boards/`）の SVG の写し。
 *
 * 字や記号で代わりにしない。 「▶」や「＋」の字は書体ごとに形と太さが変わり、
 * ボードの線の印と並ぶと1つだけ浮く。
 */
export type IconName =
  | "sidebar"
  | "back"
  | "forward"
  | "home"
  | "mic"
  | "music"
  | "wave"
  | "package"
  | "search"
  | "plus"
  | "minus"
  | "select"
  | "tree-open"
  | "tree-closed"
  | "check"
  | "retake"
  | "play-small"
  | "play"
  | "to-start"
  | "kebab";

/**
 * 1つの印の描き方。
 *
 * 線の印と塗りの印がある。 線の太さと端の形もボードのまま持つ——同じ 20 の枠でも、
 * 印ごとに 1.5〜2 で描き分けてある。
 */
type Shape = {
  viewBox: 10 | 16 | 20;
  /** 塗りの印は `null`。 */
  stroke: number | null;
  cap?: "round";
  join?: "round";
  body: ReactNode;
};

const SHAPES: Record<IconName, Shape> = {
  sidebar: {
    viewBox: 20,
    stroke: 1.5,
    body: (
      <>
        <rect x="2.5" y="4" width="15" height="12" rx="2.5" />
        <path d="M7.5 4v12" />
      </>
    ),
  },
  back: {
    viewBox: 20,
    stroke: 1.7,
    cap: "round",
    join: "round",
    body: <path d="M12.5 4.5 7 10l5.5 5.5" />,
  },
  forward: {
    viewBox: 20,
    stroke: 1.7,
    cap: "round",
    join: "round",
    body: <path d="M7.5 4.5 13 10l-5.5 5.5" />,
  },
  home: {
    viewBox: 20,
    stroke: 1.6,
    join: "round",
    body: <path d="M3 9.5 10 4l7 5.5V16a1 1 0 0 1-1 1h-3.5v-4.5h-5V17H4a1 1 0 0 1-1-1Z" />,
  },
  mic: {
    viewBox: 20,
    stroke: 1.7,
    cap: "round",
    body: (
      <>
        <rect x="7" y="2.5" width="6" height="10" rx="3" />
        <path d="M4.5 10a5.5 5.5 0 0 0 11 0M10 15.5V18" />
      </>
    ),
  },
  music: {
    viewBox: 20,
    stroke: 1.6,
    body: (
      <>
        <path d="M8 15.5V4.5l8-1.5v10.5" />
        <circle cx="6" cy="15.5" r="2" />
        <circle cx="14" cy="14" r="2" />
      </>
    ),
  },
  wave: {
    viewBox: 20,
    stroke: 1.6,
    join: "round",
    body: <path d="M2 10h2l1.5-4 2 8 2-10 2 12 2-8 1.5 4H18" />,
  },
  package: {
    viewBox: 20,
    stroke: 1.6,
    join: "round",
    body: <path d="M3 6.5 10 3l7 3.5v7L10 17l-7-3.5Z M3 6.5l7 3.5 7-3.5 M10 10v7" />,
  },
  search: {
    viewBox: 20,
    stroke: 1.7,
    cap: "round",
    body: (
      <>
        <circle cx="9" cy="9" r="5.5" />
        <path d="m13 13 4 4" />
      </>
    ),
  },
  plus: { viewBox: 20, stroke: 1.8, cap: "round", body: <path d="M10 4v12M4 10h12" /> },
  // ボードはテンポの −・＋ を字で描いている。 字で代えないので、＋ の横棒だけを使う。
  minus: { viewBox: 20, stroke: 1.8, cap: "round", body: <path d="M4 10h12" /> },
  select: {
    viewBox: 10,
    stroke: 1.5,
    cap: "round",
    join: "round",
    body: <path d="M2 3.5 L5 6.5 L8 3.5" />,
  },
  "tree-open": {
    viewBox: 16,
    stroke: 1.8,
    cap: "round",
    join: "round",
    body: <path d="M4 6l4 4 4-4" />,
  },
  "tree-closed": {
    viewBox: 16,
    stroke: 1.8,
    cap: "round",
    join: "round",
    body: <path d="M6 4l4 4-4 4" />,
  },
  check: {
    viewBox: 20,
    stroke: 2,
    cap: "round",
    join: "round",
    body: <path d="m4.5 10.5 3.5 3.5 7.5-8" />,
  },
  // 要録り直し。 ボードは描いていないので、✓ と同じ枠と太さで三角を置く。
  retake: {
    viewBox: 20,
    stroke: 2,
    join: "round",
    body: <path d="M10 4.5 16 15.5H4Z" />,
  },
  "play-small": { viewBox: 10, stroke: null, body: <path d="M2.5 1.2 L8.6 5 L2.5 8.8 Z" /> },
  play: { viewBox: 20, stroke: null, body: <path d="M6 4l10 6-10 6Z" /> },
  "to-start": {
    viewBox: 20,
    stroke: null,
    body: (
      <>
        <rect x="4" y="4" width="2.4" height="12" rx="1" />
        <path d="M16 4 7.5 10 16 16Z" />
      </>
    ),
  },
  kebab: {
    viewBox: 20,
    stroke: null,
    body: (
      <>
        <circle cx="10" cy="4.5" r="1.6" />
        <circle cx="10" cy="10" r="1.6" />
        <circle cx="10" cy="15.5" r="1.6" />
      </>
    ),
  },
};

type IconProps = {
  name: IconName;
  /** 描く大きさ（CSS ピクセル）。 置く場所ごとにボードの値を渡す。 */
  size: number;
  /** 線の太さ。 ボードが同じ印を場所で描き分けているところだけ渡す（S06 の 1.6 など）。 */
  strokeWidth?: number;
};

/**
 * 印。 大きさは置く側が渡し、色は字と同じ（`currentColor`）。
 *
 * 絵は `aria-hidden`。 意味は隣の字か、押せるものの名前が持つ。
 */
export const Icon = ({ name, size, strokeWidth }: IconProps) => {
  const shape = SHAPES[name];
  const stroked = shape.stroke !== null;
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${shape.viewBox} ${shape.viewBox}`}
      fill={stroked ? "none" : "currentColor"}
      stroke={stroked ? "currentColor" : undefined}
      strokeWidth={stroked ? (strokeWidth ?? shape.stroke ?? undefined) : undefined}
      strokeLinecap={shape.cap}
      strokeLinejoin={shape.join}
      aria-hidden="true"
      focusable="false"
      className="shrink-0"
    >
      {shape.body}
    </svg>
  );
};

/** story と検査のための一覧。 */
export const ICON_NAMES = Object.keys(SHAPES) as IconName[];
