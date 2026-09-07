import { useId } from "react";

import type { RingView, VoiceView } from "~/lib/ipc";
import { voiceStyle } from "~/lib/voice-color";

type VoiceRingsProps = {
  /** 内側から順の環。五十音の行ごとの被覆（`DEC-PLT-025`）。 */
  rings: readonly RingView[];
  /** 声から作った色。1つも録れていなければ `null`。 */
  color: VoiceView | null;
  /**
   * 図が何を表しているか。数と単位を含める（`TR-PLT-28`）。
   *
   * 渡さなければ図として読ませない。 同じ数を字で並べて置く場所では、
   * 名前を付けると的の名前が数を二度読む。
   */
  label?: string;
  /** 描き直したときに環を伸ばす。テイクが確定した直後だけ `true`。 */
  grow?: boolean;
};

/** viewBox の一辺。実寸は置く側の箱が決めるので、ここは比だけを決める。 */
const BOX = 460;

/**
 * いちばん内側の環の半径。
 *
 * 中央のボタンより外に置く。 音源の面ではここに「聴く」が座る。
 *
 * **ボタンの直径から逆算する。** `size-19`（4.75rem = 76px）なので半径は 38px。
 * いちばん小さい描画（240px）では viewBox の縮尺が 240/460 なので、
 * 38px は viewBox で 73 相当——**72 では内側の環がボタンの縁に重なり、
 * 隙間は実測 1.5px だった。** 環が「閉じているか」ではなくボタンの輪郭に
 * 見えるので、あ行の被覆が図から読めなくなっていた。**踏んだ。**
 */
const INNER_R = 88;

/**
 * いちばん外側の環の半径。
 *
 * `BOX / 2` より内側に置く。 線の太さと、質感のにじみ（`feDisplacementMap`）が
 * 外へはみ出すぶんを残しておく。
 */
const OUTER_R = 214;

/**
 * 環どうしの間隔の上限。
 *
 * **本数で決める。固定にしない。** 固定の間隔だと、環が増えたときに
 * 外側が `viewBox` を突き抜ける。SVG は外へ描かないので、
 * **環が四角く切り取られて出た**（踏んだ）。本数が少ないうちはこの値で止め、
 * 増えたら詰める。
 *
 * 本数そのものは録っても増えない。 環は五十音の行で、録音リストから決まる
 * ——15 本のまま、閉じ具合だけが変わる。
 */
const MAX_STEP = 13;

/**
 * 声の形（`DEC-PLT-025`）。
 *
 * 五十音の行ごとの同心の環で、閉じ具合が被覆。 単独音の録音リストは子音行が
 * 揃うように生成される（`TR-RCL-03`）ので、**声の形と音の地図が同じ図になる。**
 *
 * SVG で描く。canvas では描かない。 強制カラーモードでは OS が CSS の色だけを
 * 差し替え、canvas へ描いたピクセルには触らない——図が元の色のまま取り残される。
 * `stroke` は CSS の色なので、SVG なら形が残る。
 *
 * **欠けに軌道を置かない**（`docs/design/direction.md`）。 まだ録っていない
 * ぶんに薄い線を敷かない。それは「進捗バーの空白部分を薄くする」ことで、
 * 未完成を欠落として描くことになる。1本も録れていない環は描かない。
 *
 * 色は props で受ける。 部品に音源固有の色を渡さないのが規則だが、
 * 声の形はその唯一の例外（`docs/design/direction.md`）。
 *
 * 質感を許すのは産物側だけ（`docs/design/direction.md`）。 線を少し揺らす。
 * 強制カラーモードではフィルタが効かなくなるが、環の形そのものは残るので
 * 情報は失われない。
 */
export const VoiceRings = ({ rings, color, label, grow = false }: VoiceRingsProps) => {
  /*
   * 名前が無ければ、支援技術から隠す。
   *
   * `role="img"` を空の名前で残さない。 名前の無い図として読まれ、
   * axe も `role-img-alt` で落とす。
   */
  const described =
    label === undefined ? { "aria-hidden": true } : { role: "img" as const, "aria-label": label };

  // 同じ面に環が複数並ぶ（一覧）。id が衝突すると、全部が最初のフィルタを引く。
  const grain = useId();

  /*
   * 本数に合わせて詰める。
   *
   * 線の太さも一緒に細くする。 間隔だけ詰めると、隣の環と塗りがくっついて
   * 1枚の円盤になり、閉じ具合が読めなくなる。
   */
  const step =
    rings.length > 1 ? Math.min(MAX_STEP, (OUTER_R - INNER_R) / (rings.length - 1)) : MAX_STEP;
  // 太さを間隔から出す。 上限を固定にすると、間隔が詰まったときだけ
  // 塗りの占める割合が上がって、隣の環とくっつく。
  const stroke = Math.min(4, Math.max(1.6, step * 0.4));

  return (
    <svg
      viewBox={`0 0 ${BOX} ${BOX}`}
      // 一辺は置く側の箱が決める。 音源の面では余った高さに合わせて縮む。
      className="size-full"
      {...described}
      style={voiceStyle(color)}
    >
      <defs>
        <filter id={grain} x="-8%" y="-8%" width="116%" height="116%">
          <feTurbulence
            type="fractalNoise"
            baseFrequency="0.7"
            numOctaves="2"
            seed="7"
            result="n"
          />
          <feDisplacementMap
            in="SourceGraphic"
            in2="n"
            scale="1.2"
            xChannelSelector="R"
            yChannelSelector="G"
          />
        </filter>
      </defs>
      <g
        className="koeru-voice-stroke"
        fill="none"
        strokeWidth={stroke}
        strokeLinecap="round"
        filter={`url(#${grain})`}
      >
        {rings.map((ring, i) => {
          if (ring.total === 0 || ring.covered === 0) return null;
          const r = INNER_R + i * step;
          const circumference = 2 * Math.PI * r;
          const filled = (ring.covered / ring.total) * circumference;
          return (
            <circle
              // 環の同一性は内側からの順番。子音記号は画面に出す名前ではない。
              // oxlint-disable-next-line react/no-array-index-key
              key={i}
              cx={BOX / 2}
              cy={BOX / 2}
              r={r}
              /*
               * 12時から時計回りに伸ばす。
               *
               * 既定の 0 度は3時なので、回さないと右から欠ける。
               * 回転の中心を明示するのは、`transform-origin` の既定が
               * SVG 要素の原点（左上）だから。
               */
              transform={`rotate(-90 ${BOX / 2} ${BOX / 2})`}
              strokeDasharray={`${filled} ${circumference - filled}`}
              {...(grow
                ? {
                    className: "koeru-ring-grow",
                    // 伸びる前の位置。伸びきったら 0 に戻る。
                    style: { "--ring-from": `${filled}` } as React.CSSProperties,
                  }
                : {})}
            />
          );
        })}
      </g>
    </svg>
  );
};
